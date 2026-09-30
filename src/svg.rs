//! Scene to SVG, shaped like Excalidraw's `exportToSvg`: a background, one
//! group per element with its rough paths (coordinates to 2 decimals) and
//! text lines. This is the preview Keeprs stores next to each sketch.
use std::fmt::Write as _;

use crate::color::Rgba;
use crate::geometry::Affine;
use crate::render::{self, Align, FillRule, Item, Segment};
use crate::scene::Scene;

/// How to export.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgOptions {
    /// Space around the content, in scene units (Excalidraw's default 10).
    pub padding: f64,
    /// Paint the canvas background color behind the content.
    pub background: bool,
    /// Apply Excalidraw's dark filter to the whole image.
    pub dark: bool,
    /// Embed Excalifont so the file renders the same everywhere (adds about
    /// 256 KB); otherwise text names the font and relies on the viewer.
    pub embed_fonts: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            padding: 10.0,
            background: true,
            dark: false,
            embed_fonts: false,
        }
    }
}

/// `THEME_FILTER`.
const DARK_FILTER: &str = "invert(93%) hue-rotate(180deg)";

/// Returns the scene as an SVG document.
pub fn export(scene: &Scene, options: &SvgOptions) -> String {
    let elements = render::draw_order(scene);
    let [x1, y1, x2, y2] = if elements.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        crate::edit::common_bounds(elements.iter().copied())
    };
    let pad = options.padding;
    let (width, height) = (x2 - x1 + 2.0 * pad, y2 - y1 + 2.0 * pad);
    let shift = Affine::translate([pad - x1, pad - y1]);

    let mut svg = String::new();
    let filter = if options.dark {
        format!(r#" filter="{DARK_FILTER}""#)
    } else {
        String::new()
    };
    let _ = write!(
        svg,
        r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}"{filter}><!-- svg-source:roughdraft -->"#,
        w = num(width),
        h = num(height),
    );
    if options.embed_fonts {
        let font = crate::base64::encode(crate::fonts::EXCALIFONT);
        let _ = write!(
            svg,
            r#"<defs><style class="style-fonts">@font-face {{ font-family: Excalifont; src: url(data:font/ttf;base64,{font}); }}</style></defs>"#
        );
    }
    if options.background {
        let background = Rgba::parse(scene.background_color()).unwrap_or(Rgba::WHITE);
        let _ = write!(
            svg,
            r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
            num(width),
            num(height),
            hex(background)
        );
    }
    let background = scene.background_color();
    for element in elements {
        let Some(drawing) = render::render_element(element, background) else {
            continue;
        };
        let [a, b, c, d, e, f] = drawing.transform.then(shift).coefficients();
        let _ = write!(
            svg,
            r#"<g transform="matrix({} {} {} {} {} {})" stroke-linecap="round">"#,
            num(a),
            num(b),
            num(c),
            num(d),
            num(e),
            num(f)
        );
        for item in &drawing.items {
            write_item(&mut svg, item, scene);
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    svg
}

fn write_item(svg: &mut String, item: &Item, scene: &Scene) {
    match item {
        Item::Stroke {
            path,
            color,
            width,
            dash,
        } => {
            let _ = write!(
                svg,
                r#"<path fill="none" stroke="{}" stroke-width="{}""#,
                hex(*color),
                num(*width)
            );
            if color.a < 1.0 {
                let _ = write!(svg, r#" stroke-opacity="{}""#, num(f64::from(color.a)));
            }
            if let Some(dash) = dash {
                let dash: Vec<String> = dash.iter().map(|d| num(*d)).collect();
                let _ = write!(svg, r#" stroke-dasharray="{}""#, dash.join(" "));
            }
            let _ = write!(svg, r#" d="{}"/>"#, path_data(path));
        }
        Item::Fill { path, color, rule } => {
            let _ = write!(svg, r#"<path stroke="none" fill="{}""#, hex(*color));
            if color.a < 1.0 {
                let _ = write!(svg, r#" fill-opacity="{}""#, num(f64::from(color.a)));
            }
            if *rule == FillRule::EvenOdd {
                svg.push_str(r#" fill-rule="evenodd""#);
            }
            let _ = write!(svg, r#" d="{}"/>"#, path_data(path));
        }
        Item::Image {
            file_id,
            size,
            opacity,
        } => {
            let Some(url) = scene.file_data_url(file_id) else {
                return;
            };
            let _ = write!(
                svg,
                r#"<image href="{}" x="0" y="0" width="{}" height="{}" preserveAspectRatio="none""#,
                escape(url),
                num(size[0]),
                num(size[1])
            );
            if *opacity < 1.0 {
                let _ = write!(svg, r#" opacity="{}""#, num(f64::from(*opacity)));
            }
            svg.push_str("/>");
        }
        Item::Text(block) => {
            let anchor = match block.align {
                Align::Start => "start",
                Align::Middle => "middle",
                Align::End => "end",
            };
            let opacity = if block.color.a < 1.0 {
                format!(r#" fill-opacity="{}""#, num(f64::from(block.color.a)))
            } else {
                String::new()
            };
            for (i, line) in block.lines.iter().enumerate() {
                let _ = write!(
                    svg,
                    r#"<text x="{}" y="{}" font-family="{}" font-size="{}px" fill="{}"{opacity} text-anchor="{anchor}" style="white-space: pre;" direction="ltr" dominant-baseline="alphabetic">{}</text>"#,
                    num(block.x),
                    num(i as f64 * block.line_height + block.baseline),
                    font_family(block.font_family),
                    num(block.font_size),
                    hex(block.color),
                    escape(line),
                );
            }
        }
    }
}

/// rough.js `opsToPath` with 2 decimals.
fn path_data(path: &[Segment]) -> String {
    let mut d = String::new();
    for segment in path {
        match *segment {
            Segment::MoveTo([x, y]) => {
                let _ = write!(d, "M{} {} ", fixed(x), fixed(y));
            }
            Segment::LineTo([x, y]) => {
                let _ = write!(d, "L{} {} ", fixed(x), fixed(y));
            }
            Segment::CubicTo([ax, ay], [bx, by], [cx, cy]) => {
                let _ = write!(
                    d,
                    "C{} {}, {} {}, {} {} ",
                    fixed(ax),
                    fixed(ay),
                    fixed(bx),
                    fixed(by),
                    fixed(cx),
                    fixed(cy)
                );
            }
        }
    }
    d.trim_end().to_owned()
}

/// `+n.toFixed(2)`.
fn fixed(n: f64) -> String {
    num((n * 100.0).round() / 100.0)
}

/// Shortest round-trip spelling, `0` for negative zero.
fn num(n: f64) -> String {
    if n == 0.0 { "0".into() } else { n.to_string() }
}

fn hex(color: Rgba) -> String {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(color.r),
        byte(color.g),
        byte(color.b)
    )
}

/// `getFontFamilyString`: the family name plus Excalidraw's fallbacks.
fn font_family(id: u32) -> &'static str {
    match id {
        1 => "Virgil, Xiaolai, Segoe UI Emoji",
        2 => "Helvetica, Xiaolai, Segoe UI Emoji",
        3 => "Cascadia, Xiaolai, Segoe UI Emoji",
        6 => "Nunito, Xiaolai, Segoe UI Emoji",
        7 => "Lilita One, Xiaolai, Segoe UI Emoji",
        8 => "Comic Shanns, Xiaolai, Segoe UI Emoji",
        9 => "Liberation Sans, Xiaolai, Segoe UI Emoji",
        _ => "Excalifont, Xiaolai, Segoe UI Emoji",
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{fixed, path_data};
    use crate::render::Segment;

    #[test]
    fn numbers_and_paths_match_rough_js() {
        assert_eq!(fixed(1.005), "1");
        assert_eq!(fixed(-0.001), "0");
        assert_eq!(fixed(12.345_6), "12.35");
        let d = path_data(&[
            Segment::MoveTo([0.0, 1.5]),
            Segment::CubicTo([1.0, 2.0], [3.0, 4.0], [5.0, 6.004]),
        ]);
        assert_eq!(d, "M0 1.5 C1 2, 3 4, 5 6");
    }
}
