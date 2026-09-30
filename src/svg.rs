//! Scene to SVG, shaped like Excalidraw's `exportToSvg`: a background, one
//! group per element with its rough paths (coordinates to 2 decimals) and
//! text lines, frame titles and outlines, and frame children clipped to
//! their frame. Hosts can store it as a preview next to each drawing.
use std::fmt::Write as _;

use crate::color::Rgba;
use crate::geometry::{self, Affine};
use crate::render::{self, Align, Drawing, FillRule, Item, Segment};
use crate::scene::{Element, Kind, Scene};

/// How to export.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgOptions {
    /// Space around the content, in scene units (Excalidraw's default 10).
    pub padding: f64,
    /// Paint the canvas background color behind the content.
    pub background: bool,
    /// Apply Excalidraw's dark filter to the whole image.
    pub dark: bool,
    /// Embed the bundled fonts the text uses, so the file renders the same
    /// everywhere (Excalifont alone adds about 256 KB); otherwise text names
    /// the font and relies on the viewer.
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

/// Frame title color in a dark export (`nameColorDarkTheme`), before the
/// dark filter.
const DARK_FRAME_TITLE: Rgba = Rgba::rgb(
    0x7a as f32 / 255.0,
    0x7a as f32 / 255.0,
    0x7a as f32 / 255.0,
);

/// Returns the scene as an SVG document.
pub fn export(scene: &Scene, options: &SvgOptions) -> String {
    let elements = render::draw_order(scene);
    let frames = render::frames(scene);
    let [mut x1, mut y1, x2, y2] = if elements.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        crate::edit::common_bounds(elements.iter().copied())
    };
    // Frame titles sit above their frames and count as content.
    for label in elements.iter().filter_map(|e| render::frame_label(e)) {
        let [x, y] = label.transform.apply([0.0, 0.0]);
        (x1, y1) = (x1.min(x), y1.min(y));
    }
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
    svg.push_str("<defs>");
    // One clip path per frame, its rounded box (`exportToSvg`).
    for frame in elements
        .iter()
        .filter(|e| frames.contains_key(e.base.id.as_str()))
    {
        let [a, b, c, d, e, f] = geometry::element_transform(frame)
            .then(shift)
            .coefficients();
        let _ = write!(
            svg,
            r#"<clipPath id="{}"><rect ry="{r}" rx="{r}" height="{}" width="{}" transform="matrix({} {} {} {} {} {})"/></clipPath>"#,
            escape(&frame.base.id),
            num(frame.base.height),
            num(frame.base.width),
            num(a),
            num(b),
            num(c),
            num(d),
            num(e),
            num(f),
            r = num(render::FRAME_RADIUS),
        );
    }
    if options.embed_fonts {
        let used: std::collections::BTreeSet<u32> = elements
            .iter()
            .filter_map(|e| match &e.kind {
                Kind::Text(text) => Some(text.font_family),
                _ => None,
            })
            .collect();
        svg.push_str(r#"<style class="style-fonts">"#);
        for id in used {
            let Some(font) = crate::fonts::for_family(id) else {
                continue;
            };
            let name = font_family(id).split(',').next().unwrap_or_default();
            let font = crate::base64::encode(font);
            let _ = write!(
                svg,
                "@font-face {{ font-family: {name}; src: url(data:font/ttf;base64,{font}); }}"
            );
        }
        svg.push_str("</style>");
    }
    svg.push_str("</defs>");
    if options.background {
        // The colour as stored, like Excalidraw (`transparent` stays so).
        let _ = write!(
            svg,
            r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
            num(width),
            num(height),
            escape(scene.background_color())
        );
    }
    let background = scene.background_color();
    for element in elements {
        let clip = element.frame_id().filter(|id| frames.contains_key(id));
        if let Some(id) = clip {
            let _ = write!(svg, r#"<g clip-path="url(#{})">"#, escape(id));
        }
        if let Some(mut label) = render::frame_label(element) {
            for item in &mut label.items {
                if let (Item::Text(block), true) = (item, options.dark) {
                    block.color = DARK_FRAME_TITLE;
                }
            }
            write_drawing(&mut svg, &label, shift, scene);
        }
        if let Some(drawing) = render::render_element(element, background) {
            match render::arrow_label(element, scene) {
                Some(label) => write_masked(&mut svg, &drawing, shift, scene, element, label),
                None => write_drawing(&mut svg, &drawing, shift, scene),
            }
        }
        if clip.is_some() {
            svg.push_str("</g>");
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Writes one drawing as a group moved by `shift`.
fn write_drawing(svg: &mut String, drawing: &Drawing, shift: Affine, scene: &Scene) {
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
        write_item(svg, item, scene);
    }
    svg.push_str("</g>");
}

/// Writes a labelled arrow masked where its label is, followed by the mask,
/// like Excalidraw's export: a white rect from the origin to the arrow's
/// far corner plus 100, and a black one exactly the label's box.
fn write_masked(
    svg: &mut String,
    drawing: &Drawing,
    shift: Affine,
    scene: &Scene,
    element: &Element,
    label: &Element,
) {
    /// Extra room the visible part of the mask reaches past the arrow.
    const SPARE: f64 = 100.0;
    let id = escape(&element.base.id);
    let _ = write!(svg, r#"<g mask="url(#mask-{id})">"#);
    write_drawing(svg, drawing, shift, scene);
    svg.push_str("</g>");
    let [x, y] = shift.apply([element.base.x, element.base.y]);
    let [label_x, label_y] = shift.apply([label.base.x, label.base.y]);
    let _ = write!(
        svg,
        r##"<mask id="mask-{id}"><rect x="0" y="0" fill="#fff" width="{}" height="{}"/><rect x="{}" y="{}" fill="#000" width="{}" height="{}" opacity="1"/></mask>"##,
        num(element.base.width + SPARE + x),
        num(element.base.height + SPARE + y),
        num(label_x),
        num(label_y),
        num(label.base.width),
        num(label.base.height),
    );
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
        // A plain `<rect>`, like Excalidraw's frame outline.
        Item::Frame {
            size: [w, h],
            radius,
            color,
            width,
        } => {
            let _ = write!(
                svg,
                r#"<rect stroke-width="{}" stroke="{}" fill="none" ry="{r}" rx="{r}" height="{}" width="{}"/>"#,
                num(*width),
                hex(*color),
                num(*h),
                num(*w),
                r = num(*radius),
            );
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
            crop,
            flip,
        } => {
            let Some(url) = scene.file_data_url(file_id) else {
                return;
            };
            let [w, h] = *size;
            // Mirroring about the box centre, then the opacity.
            let [fx, fy] = flip.map(|f| if f { -1.0 } else { 1.0 });
            let _ = write!(
                svg,
                r#"<g transform="matrix({} 0 0 {} {} {})""#,
                num(fx),
                num(fy),
                num(if flip[0] { w } else { 0.0 }),
                num(if flip[1] { h } else { 0.0 })
            );
            if *opacity < 1.0 {
                let _ = write!(svg, r#" opacity="{}""#, num(f64::from(*opacity)));
            }
            svg.push('>');
            match crop {
                // The crop rectangle of the whole picture, stretched to the box.
                Some(crop) => {
                    let [x, y, cw, ch] = crop.rect;
                    let _ = write!(
                        svg,
                        r#"<svg x="0" y="0" width="{}" height="{}" viewBox="{} {} {} {}" preserveAspectRatio="none"><image href="{}" width="{}" height="{}"/></svg>"#,
                        num(w),
                        num(h),
                        num(x),
                        num(y),
                        num(cw),
                        num(ch),
                        escape(url),
                        num(crop.natural[0]),
                        num(crop.natural[1])
                    );
                }
                None => {
                    let _ = write!(
                        svg,
                        r#"<image href="{}" x="0" y="0" width="{}" height="{}" preserveAspectRatio="none"/>"#,
                        escape(url),
                        num(w),
                        num(h)
                    );
                }
            }
            svg.push_str("</g>");
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
        // Only Excalifont falls back to Xiaolai (`getFontFamilyFallbacks`).
        1 => "Virgil, Segoe UI Emoji",
        2 => "Helvetica, Segoe UI Emoji",
        3 => "Cascadia, Segoe UI Emoji",
        6 => "Nunito, Segoe UI Emoji",
        7 => "Lilita One, Segoe UI Emoji",
        8 => "Comic Shanns, Segoe UI Emoji",
        9 => "Liberation Sans, Segoe UI Emoji",
        _ => "Excalifont, Xiaolai, Segoe UI Emoji",
    }
}

fn escape(text: &str) -> String {
    // Control characters other than tab and newlines are not allowed in
    // XML at all.
    text.chars()
        .filter(|c| *c >= ' ' || matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{fixed, path_data};
    use crate::render::Segment;

    #[test]
    fn a_labelled_arrow_is_masked_where_its_label_is() {
        let base = r##""angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"seed":1,"isDeleted":false"##;
        let json = format!(
            r##"{{"type":"excalidraw","elements":[
                {{"id":"r","type":"arrow","x":100,"y":50,"width":200,"height":0,{base},"points":[[0,0],[200,0]],"boundElements":[{{"id":"t","type":"text"}}]}},
                {{"id":"t","type":"text","x":170,"y":37.5,"width":60,"height":25,{base},"text":"hi","fontSize":20,"fontFamily":5,"textAlign":"center","verticalAlign":"middle","lineHeight":1.25,"containerId":"r"}}
            ]}}"##
        );
        let scene: crate::scene::Scene = serde_json::from_str(&json).unwrap();
        let svg = super::export(&scene, &super::SvgOptions::default());
        assert!(svg.contains(r#"<g mask="url(#mask-r)">"#), "{svg}");
        // Export padding 10: the arrow starts at (10, y) and the label's box
        // at (80, y), so the visible rect is 200 + 100 + 10 wide.
        let mask = &svg[svg.find(r#"<mask id="mask-r">"#).expect("the mask")..];
        assert!(
            mask.starts_with(r##"<mask id="mask-r"><rect x="0" y="0" fill="#fff" width="310""##),
            "{mask}"
        );
        assert!(
            mask.contains(r##"<rect x="80" "##)
                && mask.contains(r##"fill="#000" width="60" height="25""##),
            "{mask}"
        );
    }

    #[test]
    fn writes_the_background_as_stored_and_drops_xml_control_characters() {
        let json = r##"{"type":"excalidraw","elements":[{"id":"t","type":"text","x":0,"y":0,"width":20,"height":25,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"seed":1,"isDeleted":false,"text":"a\u000bb","fontSize":20,"fontFamily":6,"textAlign":"left","verticalAlign":"top","lineHeight":1.25}],"appState":{"viewBackgroundColor":"transparent"}}"##;
        let scene: crate::scene::Scene = serde_json::from_str(json).unwrap();
        let svg = super::export(&scene, &super::SvgOptions::default());
        assert!(svg.contains(r#"fill="transparent"/>"#), "{svg}");
        assert!(svg.contains(">ab</text>") && !svg.contains('\u{b}'));
        assert!(svg.contains(r#"font-family="Nunito, Segoe UI Emoji""#));
    }

    #[test]
    fn embeds_each_bundled_font_the_text_uses() {
        let json = std::fs::read_to_string("tests/fixtures/scenes/fonts.excalidraw").unwrap();
        let mut scene: crate::scene::Scene = serde_json::from_str(&json).unwrap();
        let options = super::SvgOptions {
            embed_fonts: true,
            ..Default::default()
        };
        let svg = super::export(&scene, &options);
        for name in [
            "Virgil",
            "Excalifont",
            "Nunito",
            "Lilita One",
            "Comic Shanns",
        ] {
            assert!(
                svg.contains(&format!(
                    "@font-face {{ font-family: {name}; src: url(data:font/ttf;base64,"
                )),
                "{name}"
            );
        }
        scene.elements.retain(|e| e.base.id == "t6");
        let svg = super::export(&scene, &options);
        assert_eq!(svg.matches("@font-face").count(), 1, "only Nunito");
    }

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
