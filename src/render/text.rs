//! Text elements as positioned lines, with Excalidraw's per-font vertical
//! metrics (`fonts/FontMetadata.ts`).
use super::{Align, TextBlock, color};
use crate::scene::{Element, Text, TextAlign};

pub(super) fn text_block(element: &Element, text: &Text, opacity: f32) -> TextBlock {
    let line_height = text.font_size * text.line_height;
    // `getVerticalOffset`: center the font's ascent + descent in the line.
    let metrics = font_metrics(text.font_family);
    let em = text.font_size / metrics.units_per_em;
    let gap = (line_height - em * metrics.ascender + em * metrics.descender) / 2.0;
    let (x, align) = match text.text_align {
        TextAlign::Center => (element.base.width / 2.0, Align::Middle),
        TextAlign::Right => (element.base.width, Align::End),
        // ponytail: RTL lines anchor at the start, Excalidraw anchors them at the end
        TextAlign::Left | TextAlign::Other(_) => (0.0, Align::Start),
    };
    TextBlock {
        lines: text
            .text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(String::from)
            .collect(),
        x,
        line_height,
        baseline: em * metrics.ascender + gap,
        align,
        font_family: text.font_family,
        font_size: text.font_size,
        color: color(&element.base.stroke_color, opacity),
    }
}

/// Vertical font metrics from `fonts/FontMetadata.ts` (hhea table values).
struct FontMetrics {
    units_per_em: f64,
    ascender: f64,
    descender: f64,
}

fn font_metrics(family: u32) -> FontMetrics {
    let (units_per_em, ascender, descender) = match family {
        6 => (1000.0, 1011.0, -353.0), // Nunito
        7 => (1000.0, 923.0, -220.0),  // Lilita One
        8 => (1000.0, 750.0, -250.0),  // Comic Shanns
        2 => (2048.0, 1577.0, -471.0), // Helvetica
        3 => (2048.0, 1900.0, -480.0), // Cascadia
        9 => (2048.0, 1854.0, -434.0), // Liberation Sans
        _ => (1000.0, 886.0, -374.0),  // Excalifont, Virgil, and the fallback
    };
    FontMetrics {
        units_per_em,
        ascender,
        descender,
    }
}
