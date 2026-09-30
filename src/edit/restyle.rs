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

// Private API
impl Editor {
    /// Remembers the style of the first selected element in the stack and
    /// of its label (`actionCopyStyles`).
    pub(super) fn copy_styles(&mut self) {
        let Some(element) = self
            .scene
            .elements
            .iter()
            .find(|e| self.selected.contains(&e.base.id))
        else {
            return;
        };
        let label = self
            .scene
            .elements
            .iter()
            .find(|e| !e.base.is_deleted && container_id(e) == Some(element.base.id.as_str()))
            .cloned();
        self.copied_style = Some((element.clone(), label));
    }

    /// Applies the remembered style to the selection and its labels, as one
    /// undo step (`actionPasteStyles`): labels take the copied label's.
    pub(super) fn paste_styles(&mut self) {
        let Some((source, source_label)) = self.copied_style.clone() else {
            return;
        };
        let before = self.scene.elements.clone();
        let mut texts = Vec::new();
        for (index, _) in self.moving() {
            let element = &mut self.scene.elements[index];
            let from = if container_id(element).is_some() {
                match &source_label {
                    Some(label) => label,
                    None => continue,
                }
            } else {
                &source
            };
            let base = &mut element.base;
            base.background_color = from.base.background_color.clone();
            base.stroke_width = from.base.stroke_width;
            base.stroke_color = from.base.stroke_color.clone();
            base.stroke_style = from.base.stroke_style.clone();
            base.fill_style = from.base.fill_style.clone();
            base.opacity = from.base.opacity;
            base.roughness = from.base.roughness;
            base.roundness = from
                .base
                .roundness
                .as_ref()
                .and_then(|r| roundness_for(&element.kind, r));
            match (&mut element.kind, &from.kind) {
                (Kind::Text(text), Kind::Text(from)) => {
                    text.font_size = from.font_size;
                    text.font_family = from.font_family;
                    text.text_align = from.text_align.clone();
                    text.line_height = from.line_height;
                }
                (Kind::Arrow(line), Kind::Arrow(from)) => {
                    line.start_arrowhead = from.start_arrowhead.clone();
                    line.end_arrowhead = from.end_arrowhead.clone();
                }
                _ => {}
            }
            if matches!(element.kind, Kind::Text(_)) {
                texts.push((index, element.original_text().to_owned()));
            }
            if element != &before[index] {
                element.touch();
            }
        }
        for (index, original) in texts {
            self.layout_text(index, &original);
        }
        if before != self.scene.elements {
            self.history.record(before);
        }
    }

    /// Multiplies the font size of selected text and labels by `factor`,
    /// rounded to whole sizes (`changeFontSize`), as one undo step. New
    /// text takes the size too when every changed element ends up equal.
    pub(super) fn step_font_size(&mut self, factor: f64) {
        self.finish_text();
        let before = self.scene.elements.clone();
        let mut sizes = Vec::new();
        for (index, _) in self.moving() {
            let element = &mut self.scene.elements[index];
            let Kind::Text(text) = &mut element.kind else {
                continue;
            };
            text.font_size = (text.font_size * factor).round();
            sizes.push(text.font_size);
            let original = element.original_text().to_owned();
            element.touch();
            self.layout_text(index, &original);
        }
        if let Some(&size) = sizes.first()
            && sizes.iter().all(|s| *s == size)
        {
            self.style.font_size = size;
        }
        if before != self.scene.elements {
            self.history.record(before);
        }
    }
}

/// The copied rounding if this kind of element uses that kind of radius,
/// else its own default (`canApplyRoundnessTypeToElement`,
/// `getDefaultRoundnessTypeForElement`).
fn roundness_for(kind: &Kind, copied: &Roundness) -> Option<Roundness> {
    /// `ROUNDNESS.LEGACY`.
    const LEGACY: u8 = 1;
    let adaptive =
        matches!(kind, Kind::Rectangle) || matches!(kind, Kind::Other(t) if t == "image");
    let proportional = matches!(kind, Kind::Line(_) | Kind::Arrow(_) | Kind::Diamond);
    let fits = match copied.kind {
        ADAPTIVE | LEGACY => adaptive,
        PROPORTIONAL => proportional,
        _ => false,
    };
    if fits {
        Some(copied.clone())
    } else if proportional {
        Some(Roundness {
            kind: PROPORTIONAL,
            value: None,
        })
    } else if adaptive {
        Some(Roundness {
            kind: ADAPTIVE,
            value: None,
        })
    } else {
        None
    }
}
