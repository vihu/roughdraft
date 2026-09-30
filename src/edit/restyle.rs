//! Style panel changes applied to the selection (`actions/actionProperties.tsx`).
use super::{Editor, Style, StyleChange};
use crate::hit::container_id;
use crate::scene::{Kind, Roundness};

/// `ROUNDNESS.PROPORTIONAL_RADIUS`.
const PROPORTIONAL: u8 = 2;

/// `ROUNDNESS.ADAPTIVE_RADIUS`.
const ADAPTIVE: u8 = 3;

impl Editor {
    /// Returns the style the panel shows: the first selected element's, or
    /// the style for new elements.
    pub fn current_style(&self) -> Style {
        let first = self.selection().find(|e| container_id(e).is_none());
        first.map_or_else(|| self.style.clone(), |element| self.style.of(element))
    }

    /// Applies a style change to new elements and to the selection (labels
    /// of selected shapes included), as one undo step. Properties that do
    /// not apply to an element's type are skipped.
    pub fn apply_style(&mut self, change: StyleChange) {
        self.finish_text();
        self.style.apply(&change);
        let before = self.scene.elements.clone();
        for (index, _) in self.moving() {
            let element = &mut self.scene.elements[index];
            let is_text = matches!(element.kind, Kind::Text(_));
            let base = &mut element.base;
            match &change {
                StyleChange::StrokeColor(color) => base.stroke_color = color.clone(),
                StyleChange::Opacity(opacity) => base.opacity = *opacity,
                _ if is_text => {}
                StyleChange::BackgroundColor(color) => base.background_color = color.clone(),
                StyleChange::FillStyle(fill) => base.fill_style = fill.clone(),
                StyleChange::StrokeWidth(width) => base.stroke_width = *width,
                StyleChange::StrokeStyle(style) => base.stroke_style = style.clone(),
                StyleChange::Roughness(roughness) => {
                    base.roughness = *roughness;
                    base.seed = crate::random::integer();
                }
                _ => {}
            }
            match (&change, &mut element.kind) {
                (
                    StyleChange::RoundEdges(round),
                    Kind::Rectangle | Kind::Diamond | Kind::Line(_),
                ) => {
                    let kind = if matches!(element.kind, Kind::Rectangle) {
                        ADAPTIVE
                    } else {
                        PROPORTIONAL
                    };
                    element.base.roundness = round.then_some(Roundness { kind, value: None });
                }
                (StyleChange::StartArrowhead(head), Kind::Arrow(line)) => {
                    line.start_arrowhead = head.clone()
                }
                (StyleChange::EndArrowhead(head), Kind::Arrow(line)) => {
                    line.end_arrowhead = head.clone()
                }
                (StyleChange::FontSize(size), Kind::Text(text)) => text.font_size = *size,
                (StyleChange::FontFamily(family), Kind::Text(text)) => {
                    text.font_family = *family;
                    text.line_height = super::text::line_height(*family);
                }
                (StyleChange::TextAlign(align), Kind::Text(text))
                    if text.container_id.is_none() =>
                {
                    text.text_align = align.clone();
                }
                _ => {}
            }
            if element != &before[index] {
                element.touch();
            }
        }
        if matches!(
            change,
            StyleChange::FontSize(_) | StyleChange::FontFamily(_)
        ) {
            let texts: Vec<(usize, String)> = self
                .moving()
                .into_iter()
                .filter(|(i, _)| matches!(self.scene.elements[*i].kind, Kind::Text(_)))
                .map(|(i, _)| (i, self.scene.elements[i].original_text().to_owned()))
                .collect();
            for (index, original) in texts {
                self.layout_text(index, &original);
            }
        }
        if before != self.scene.elements {
            self.history.record(before);
        }
    }
}
