//! Our drawings must match Excalidraw 0.18.1's own SVG export, mark for mark.
//!
//! Every `tests/fixtures/scenes/<name>.excalidraw` has `<name>.svg` next to
//! it, rendered by real Excalidraw 0.18.1 through the `excalidraw-image` CLI:
//! `excalidraw-image <name>.excalidraw -o <name>.svg --non-editable`.
//!
//! Compared per path and text line, in draw order: path commands and
//! coordinates (Excalidraw rounds to 2 decimals), stroke and fill colors with
//! opacity, stroke width, dash pattern, element transform, and text position,
//! anchor, size and content. Keeprs memos from `ROUGHDRAFT_EXTRA_FIXTURES`
//! are compared against the SVG Keeprs stored with them.
mod common;

use std::path::Path;

use roughdraft::color::Rgba;
use roughdraft::geometry::Affine;
use roughdraft::render::{self, Align, Item, Segment};
use roughdraft::scene::Scene;

/// Excalidraw writes coordinates with `toFixed(2)`.
const COORDINATE_TOLERANCE: f64 = 0.0051;

/// Transforms print full precision; allow float noise only.
const TRANSFORM_TOLERANCE: f64 = 1e-6;

/// One 8-bit color step.
const COLOR_TOLERANCE: f32 = 0.5 / 255.0;

#[test]
fn matches_excalidraw_svg_export() {
    let mut failures = Vec::new();
    for fixture in common::fixtures() {
        let Some(svg) = &fixture.svg else {
            continue;
        };
        let scene: Scene = serde_json::from_value(fixture.scene).unwrap();
        eprintln!("{}: {} marks", fixture.name, svg_marks(svg).len());
        let name = &fixture.name;
        failures.extend(
            compare(&scene, svg)
                .into_iter()
                .map(|f| format!("{name}: {f}")),
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Our SVG export must carry the same marks as Excalidraw's export.
#[test]
fn our_export_matches_excalidraw_export() {
    let mut failures = Vec::new();
    for fixture in common::fixtures() {
        let Some(reference) = &fixture.svg else {
            continue;
        };
        let scene: Scene = serde_json::from_value(fixture.scene).unwrap();
        let ours = roughdraft::svg::export(&scene, &roughdraft::svg::SvgOptions::default());
        let name = &fixture.name;
        failures.extend(
            compare_marks(svg_marks(reference), svg_marks(&ours))
                .into_iter()
                .map(|f| format!("{name}: {f}")),
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn detects_a_changed_seed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenes");
    let mut scene: Scene =
        serde_json::from_str(&std::fs::read_to_string(dir.join("l0-coverage.excalidraw")).unwrap())
            .unwrap();
    let svg = std::fs::read_to_string(dir.join("l0-coverage.svg")).unwrap();
    scene.elements[0].base.seed += 1;
    let failures = compare(&scene, &svg);
    assert!(
        failures.iter().any(|f| f.starts_with("mark 0: path")),
        "{failures:?}"
    );
}

/// A path or text line, in scene-export units.
#[derive(Debug)]
enum Mark {
    Stroke {
        d: Vec<Token>,
        color: Rgba,
        width: f64,
        dash: Vec<f64>,
    },
    Fill {
        d: Vec<Token>,
        color: Rgba,
    },
    Text {
        x: f64,
        y: f64,
        anchor: &'static str,
        size: f64,
        color: Rgba,
        content: String,
    },
}

#[derive(Debug, PartialEq)]
enum Token {
    Command(char),
    Number(f64),
}

fn compare(scene: &Scene, svg: &str) -> Vec<String> {
    compare_marks(svg_marks(svg), our_marks(scene))
}

fn compare_marks(want: Vec<(Affine, Mark)>, got: Vec<(Affine, Mark)>) -> Vec<String> {
    if want.len() != got.len() {
        return vec![format!(
            "{} marks in the SVG, {} drawn",
            want.len(),
            got.len()
        )];
    }
    // Only images (compared by pixels instead): nothing to line up.
    if want.is_empty() {
        return Vec::new();
    }
    // Excalidraw's export shifts everything by one offset (bounds + padding).
    let offset = {
        let ([wx, wy], [gx, gy]) = (want[0].0.apply([0.0, 0.0]), got[0].0.apply([0.0, 0.0]));
        Affine::translate([wx - gx, wy - gy])
    };

    let mut failures = Vec::new();
    for (i, ((want_t, want), (got_t, got))) in want.iter().zip(&got).enumerate() {
        let got_t = got_t.then(offset);
        for probe in [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]] {
            let (w, g) = (want_t.apply(probe), got_t.apply(probe));
            if (w[0] - g[0]).abs() > TRANSFORM_TOLERANCE
                || (w[1] - g[1]).abs() > TRANSFORM_TOLERANCE
            {
                failures.push(format!(
                    "mark {i}: transform maps {probe:?} to {w:?}, we map it to {g:?}"
                ));
                break;
            }
        }
        if let Some(why) = mark_mismatch(want, got) {
            failures.push(format!("mark {i}: {why}"));
        }
    }
    failures
}

fn mark_mismatch(want: &Mark, got: &Mark) -> Option<String> {
    let colors = |w: &Rgba, g: &Rgba| {
        let pairs = [(w.r, g.r), (w.g, g.g), (w.b, g.b), (w.a, g.a)];
        (!pairs.iter().all(|(w, g)| (w - g).abs() <= COLOR_TOLERANCE))
            .then(|| format!("color {w:?} != {g:?}"))
    };
    match (want, got) {
        (
            Mark::Stroke {
                d: wd,
                color: wc,
                width: ww,
                dash: wdash,
            },
            Mark::Stroke {
                d: gd,
                color: gc,
                width: gw,
                dash: gdash,
            },
        ) => path_mismatch(wd, gd)
            .or_else(|| colors(wc, gc))
            .or_else(|| (ww != gw).then(|| format!("stroke width {ww} != {gw}")))
            .or_else(|| (wdash != gdash).then(|| format!("dash {wdash:?} != {gdash:?}"))),
        (Mark::Fill { d: wd, color: wc }, Mark::Fill { d: gd, color: gc }) => {
            path_mismatch(wd, gd).or_else(|| colors(wc, gc))
        }
        (
            Mark::Text {
                x: wx,
                y: wy,
                anchor: wa,
                size: ws,
                color: wc,
                content: wt,
            },
            Mark::Text {
                x: gx,
                y: gy,
                anchor: ga,
                size: gs,
                color: gc,
                content: gt,
            },
        ) => {
            let position =
                (wx - gx).abs() > TRANSFORM_TOLERANCE || (wy - gy).abs() > TRANSFORM_TOLERANCE;
            (position || wa != ga || (ws - gs).abs() > TRANSFORM_TOLERANCE || wt != gt)
                .then(|| format!("text {wt:?} at ({wx}, {wy}) {wa} {ws}px != {gt:?} at ({gx}, {gy}) {ga} {gs}px"))
                .or_else(|| colors(wc, gc))
        }
        _ => Some(format!("kind differs: {want:?} vs {got:?}")),
    }
}

fn path_mismatch(want: &[Token], got: &[Token]) -> Option<String> {
    if want.len() != got.len() {
        return Some(format!(
            "path has {} tokens, ours {}",
            want.len(),
            got.len()
        ));
    }
    for (i, pair) in want.iter().zip(got).enumerate() {
        let same = match pair {
            (Token::Command(w), Token::Command(g)) => w == g,
            (Token::Number(w), Token::Number(g)) => (w - g).abs() <= COORDINATE_TOLERANCE,
            _ => false,
        };
        if !same {
            return Some(format!("path token {i}: {:?} != {:?}", pair.0, pair.1));
        }
    }
    None
}

fn our_marks(scene: &Scene) -> Vec<(Affine, Mark)> {
    let tokens = |path: &[Segment]| {
        let mut out = Vec::new();
        for segment in path {
            let (command, points): (char, Vec<[f64; 2]>) = match *segment {
                Segment::MoveTo(p) => ('M', vec![p]),
                Segment::LineTo(p) => ('L', vec![p]),
                Segment::CubicTo(a, b, c) => ('C', vec![a, b, c]),
            };
            out.push(Token::Command(command));
            out.extend(points.into_iter().flatten().map(Token::Number));
        }
        out
    };
    let mut marks = Vec::new();
    for drawing in render::render(scene) {
        for item in drawing.items {
            match item {
                Item::Stroke {
                    path,
                    color,
                    width,
                    dash,
                } => marks.push((
                    drawing.transform,
                    Mark::Stroke {
                        d: tokens(&path),
                        color,
                        width,
                        dash: dash.unwrap_or_default(),
                    },
                )),
                Item::Fill { path, color, .. } => {
                    marks.push((
                        drawing.transform,
                        Mark::Fill {
                            d: tokens(&path),
                            color,
                        },
                    ));
                }
                // Excalidraw exports images as <use> of a <symbol>; not compared.
                Item::Image { .. } => {}
                Item::Text(block) => {
                    for (i, line) in block.lines.into_iter().enumerate() {
                        let anchor = match block.align {
                            Align::Start => "start",
                            Align::Middle => "middle",
                            Align::End => "end",
                        };
                        marks.push((
                            drawing.transform,
                            Mark::Text {
                                x: block.x,
                                y: i as f64 * block.line_height + block.baseline,
                                anchor,
                                size: block.font_size,
                                color: block.color,
                                content: line,
                            },
                        ));
                    }
                }
            }
        }
    }
    marks
}

fn svg_marks(svg: &str) -> Vec<(Affine, Mark)> {
    let doc = roxmltree::Document::parse(svg).unwrap();
    let mut marks = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if !matches!(tag, "path" | "text")
            || node
                .ancestors()
                .any(|a| matches!(a.tag_name().name(), "defs" | "mask"))
        {
            continue;
        }
        // Nearest ancestor attribute, like CSS inheritance for presentation attributes.
        let inherited = |name: &str| node.ancestors().find_map(|a| a.attribute(name));
        let opacity = |name: &str| inherited(name).map_or(1.0, |v| v.parse::<f32>().unwrap());
        let color = |css: &str, opacity: f32| {
            Rgba::parse(css)
                .unwrap_or_else(|| panic!("color {css}"))
                .fade(opacity)
        };
        let transform =
            parse_transform(inherited("transform").expect("marks sit in a transformed group"));
        let mark = if tag == "text" {
            let anchor = match node.attribute("text-anchor") {
                Some("middle") => "middle",
                Some("end") => "end",
                _ => "start",
            };
            Mark::Text {
                x: number(node.attribute("x").unwrap()),
                y: number(node.attribute("y").unwrap()),
                anchor,
                size: number(node.attribute("font-size").unwrap().trim_end_matches("px")),
                color: color(node.attribute("fill").unwrap(), opacity("fill-opacity")),
                content: node.text().unwrap_or_default().to_owned(),
            }
        } else if node.attribute("fill") == Some("none") {
            let dash = node
                .attribute("stroke-dasharray")
                .map(|d| d.split_whitespace().map(number).collect());
            Mark::Stroke {
                d: parse_path(node.attribute("d").unwrap()),
                color: color(node.attribute("stroke").unwrap(), opacity("stroke-opacity")),
                width: number(node.attribute("stroke-width").unwrap()),
                dash: dash.unwrap_or_default(),
            }
        } else {
            Mark::Fill {
                d: parse_path(node.attribute("d").unwrap()),
                color: color(node.attribute("fill").unwrap(), opacity("fill-opacity")),
            }
        };
        marks.push((transform, mark));
    }
    marks
}

fn number(text: &str) -> f64 {
    text.parse()
        .unwrap_or_else(|_| panic!("not a number: {text:?}"))
}

/// Parses Excalidraw's `translate(x y) rotate(deg cx cy)`.
fn parse_transform(text: &str) -> Affine {
    let numbers: Vec<f64> = text
        .split(['(', ')'])
        .skip(1)
        .step_by(2)
        .flat_map(str::split_whitespace)
        .map(number)
        .collect();
    if let [a, b, c, d, e, f] = numbers[..] {
        return Affine::from_coefficients([a, b, c, d, e, f]);
    }
    let [x, y, degrees, cx, cy] = numbers[..] else {
        panic!("unexpected transform {text:?}");
    };
    Affine::rotate_about(degrees.to_radians(), [cx, cy]).then(Affine::translate([x, y]))
}

/// Splits rough.js' `opsToPath` output (`M0 0 C1 2, 3 4, 5 6 L7 8`).
fn parse_path(d: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, tokens: &mut Vec<Token>| {
        if !current.is_empty() {
            tokens.push(Token::Number(number(current)));
            current.clear();
        }
    };
    for c in d.chars() {
        match c {
            'M' | 'C' | 'L' => {
                flush(&mut current, &mut tokens);
                tokens.push(Token::Command(c));
            }
            ' ' | ',' => flush(&mut current, &mut tokens),
            _ => current.push(c),
        }
    }
    flush(&mut current, &mut tokens);
    tokens
}
