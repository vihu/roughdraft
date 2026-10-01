//! Commands for agents and scripts (PLAN-002): `build`, `check` and
//! `render`. They never prompt, print a JSON result on stdout, and exit
//! non-zero on failure.
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const HELP: &str = "\
roughdraft: an editor for Excalidraw diagrams, and commands for scripts.

Usage:
  roughdraft [file.excalidraw] [--dark]      open the editor
  roughdraft build <skeleton.json> -o <out.excalidraw>
                                             build a complete Excalidraw file
                                             from a skeleton (\"-\" reads stdin)
  roughdraft check <file.excalidraw>         list what to fix: labels that do
                                             not fit, broken bindings, overlaps
  roughdraft render <file.excalidraw> -o <out.png|out.svg|out.html>
                    [--dark] [--scale 2] [--title T]
                                             draw it as Excalidraw exports it,
                                             no window needed: a PNG, an SVG
                                             with its fonts, or one
                                             self-contained web page with a
                                             light/dark switch; --dark draws
                                             the PNG or SVG dark, and starts
                                             the page dark instead of following
                                             the system
  roughdraft --version                       print the version (also -V)
  roughdraft help                            show this

Each command prints JSON on stdout. build: {\"ok\": true, \"shapes\": {id: [x, y,
width, height]}, ...} or
{\"ok\": false, \"errors\": [{\"path\", \"message\"}]}. check: {\"ok\", \"problems\":
[{\"kind\", \"severity\", \"ids\", \"message\"}]}, ok when no errors. render:
{\"ok\": true, \"output\", \"format\", \"bytes\"}, plus \"width\", \"height\", \"scale\" and
\"renderer\" for a PNG. Exit codes:
0 done, 1 check found errors or render failed, 2 bad input.";

/// Pixels per scene unit for `render`, as Excalidraw's 2x PNG export.
const DEFAULT_SCALE: f32 = 2.0;

/// The scales `render` takes.
const SCALE: std::ops::RangeInclusive<f32> = 0.1..=8.0;

/// Runs the command in `args`; `None` when there is none, to open the
/// editor.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    Some(match command.as_str() {
        "build" => build(rest),
        "check" => check(rest),
        "render" => render(rest),
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            0
        }
        "--version" | "-V" => {
            println!("roughdraft {}", env!("CARGO_PKG_VERSION"));
            0
        }
        _ => return None,
    })
}

/// `roughdraft build <skeleton.json> -o <out.excalidraw>`.
fn build(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return 0;
    }
    let (input, output) = match input_and_output(args) {
        Ok(paths) => paths,
        Err(message) => return fail(&message),
    };
    let text = match read(&input) {
        Ok(text) => text,
        Err(message) => return fail(&message),
    };
    let skeleton: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => return fail(&format!("{}: not JSON: {e}", input.display())),
    };
    match roughdraft::build::build(&skeleton, roughdraft::widget::font_measure()) {
        Ok(built) => {
            if let Err(e) = std::fs::write(&output, built.scene.to_json()) {
                return fail(&format!("{}: {e}", output.display()));
            }
            print(json!({
                "ok": true,
                "output": output,
                "elements": built.scene.elements.len(),
                "shapes": shapes(&built.scene),
                "warnings": built.warnings.iter().map(|w| json!({"path": w.path, "message": w.message})).collect::<Vec<_>>(),
            }));
            0
        }
        Err(issues) => {
            print(json!({
                "ok": false,
                "errors": issues.iter().map(|i| json!({"path": i.path, "message": i.message})).collect::<Vec<_>>(),
            }));
            2
        }
    }
}

/// `roughdraft check <file.excalidraw>`.
fn check(args: &[String]) -> i32 {
    let path = match args {
        [flag] if flag == "--help" || flag == "-h" => {
            println!("{HELP}");
            return 0;
        }
        [path] => PathBuf::from(path),
        _ => return fail("usage: roughdraft check <file.excalidraw> (or - for stdin)"),
    };
    let text = match read(&path) {
        Ok(text) => text,
        Err(message) => return fail(&message),
    };
    let file: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => return fail(&format!("{}: not JSON: {e}", path.display())),
    };
    let measure = roughdraft::widget::font_measure();
    let problems = match roughdraft::check::check(&file, &*measure) {
        Ok(problems) => problems,
        Err(e) => return fail(&format!("{}: not an Excalidraw scene: {e}", path.display())),
    };
    let errors = problems
        .iter()
        .filter(|p| p.severity == roughdraft::check::Severity::Error)
        .count();
    print(json!({
        "ok": errors == 0,
        "errors": errors,
        "warnings": problems.len() - errors,
        "problems": problems.iter().map(|p| json!({
            "kind": p.kind,
            "severity": match p.severity {
                roughdraft::check::Severity::Error => "error",
                roughdraft::check::Severity::Warning => "warning",
            },
            "ids": p.ids,
            "message": p.message,
        })).collect::<Vec<_>>(),
    }));
    i32::from(errors > 0)
}

/// `roughdraft render <file.excalidraw> -o <out.png|.svg|.html> [--dark]
/// [--scale 2] [--title T]`: the format follows the output's extension.
fn render(args: &[String]) -> i32 {
    use roughdraft::widget::Appearance;
    let mut appearance = Appearance::Light;
    let mut scale = None;
    let mut title = None;
    let mut paths = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return 0;
            }
            "--dark" => appearance = Appearance::Dark,
            "--scale" => match args.next().and_then(|s| s.parse().ok()) {
                Some(value) if SCALE.contains(&value) => scale = Some(value),
                _ => return fail(&format!("--scale takes a number in {SCALE:?}")),
            },
            "--title" => match args.next() {
                Some(text) => title = Some(text.clone()),
                None => return fail("--title needs the page's title"),
            },
            _ => paths.push(arg.clone()),
        }
    }
    let (input, output) = match input_and_output(&paths) {
        Ok(paths) => paths,
        Err(message) => return fail(&message),
    };
    let format = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if !matches!(format, "png" | "svg" | "html") {
        return fail("the output must end in .png, .svg or .html");
    }
    if scale.is_some() && format != "png" {
        return fail("--scale applies to .png only: SVG and HTML scale without loss");
    }
    if title.is_some() && format != "html" {
        return fail("--title applies to .html only");
    }
    let text = match read(&input) {
        Ok(text) => text,
        Err(message) => return fail(&message),
    };
    let scene: roughdraft::scene::Scene = match serde_json::from_str(&text) {
        Ok(scene) => scene,
        Err(e) => {
            return fail(&format!(
                "{}: not an Excalidraw scene: {e}",
                input.display()
            ));
        }
    };
    let (bytes, details) = match format {
        "png" => {
            match roughdraft::widget::render_png(scene, appearance, scale.unwrap_or(DEFAULT_SCALE))
            {
                Ok(png) => (
                    png.bytes,
                    json!({"width": png.width, "height": png.height, "scale": png.scale, "renderer": png.renderer}),
                ),
                Err(e) => {
                    print(json!({"ok": false, "errors": [{"path": "", "message": e.to_string()}]}));
                    return 1;
                }
            }
        }
        "svg" => {
            let options = roughdraft::svg::SvgOptions {
                dark: appearance == Appearance::Dark,
                ..Default::default()
            };
            (
                roughdraft::svg::export(&scene.saved(), &options).into_bytes(),
                json!({}),
            )
        }
        _ => {
            let stem = input
                .file_stem()
                .filter(|_| input != Path::new("-"))
                .map_or("diagram".into(), |s| s.to_string_lossy().into_owned());
            let title = title.unwrap_or_else(|| stem.clone());
            let start = match appearance {
                Appearance::Light => roughdraft::html::StartTheme::System,
                Appearance::Dark => roughdraft::html::StartTheme::Dark,
            };
            let page = roughdraft::html::page(&scene, &title, &format!("{stem}.excalidraw"), start);
            (page.into_bytes(), json!({}))
        }
    };
    if let Err(e) = std::fs::write(&output, &bytes) {
        return fail(&format!("{}: {e}", output.display()));
    }
    let mut result = json!({"ok": true, "output": output, "format": format, "bytes": bytes.len()});
    if let (Some(result), Value::Object(details)) = (result.as_object_mut(), details) {
        result.extend(details);
    }
    print(result);
    0
}

/// Where each shape, free text and frame landed, as `[x, y, width,
/// height]` by id: what an agent needs to route arrows with points.
fn shapes(scene: &roughdraft::scene::Scene) -> serde_json::Map<String, Value> {
    use roughdraft::scene::Kind;
    scene
        .elements
        .iter()
        .filter(|e| match &e.kind {
            Kind::Arrow(_) | Kind::Line(_) => false,
            Kind::Text(text) => text.container_id.is_none(),
            _ => true,
        })
        .map(|e| {
            let b = &e.base;
            let place = [b.x, b.y, b.width, b.height].map(|v| v.round() as i64);
            (b.id.clone(), json!(place))
        })
        .collect()
}

/// The input path (or `-` for stdin) and the `-o` path.
fn input_and_output(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let mut input = None;
    let mut output = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = Some(args.next().ok_or("-o needs a path")?);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option {other}"));
            }
            other if input.is_none() => input = Some(other),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    let input = input.ok_or("missing the input file (or - for stdin)")?;
    let output = output.ok_or("missing -o <output file>")?;
    Ok((PathBuf::from(input), PathBuf::from(output)))
}

fn read(path: &Path) -> Result<String, String> {
    if path == Path::new("-") {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|e| format!("stdin: {e}"))?;
        return Ok(text);
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reports a failure outside the input (usage, files) and returns its exit
/// code.
fn fail(message: &str) -> i32 {
    print(json!({"ok": false, "errors": [{"path": "", "message": message}]}));
    2
}

/// Prints `value` as indented JSON, with each array of numbers or strings
/// on one line (shape boxes, ids).
fn print(value: Value) {
    let pretty = serde_json::to_string_pretty(&value).expect("JSON values serialize");
    let lines: Vec<&str> = pretty.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.ends_with('[') {
            let items: Vec<&str> = lines[i + 1..]
                .iter()
                .map(|l| l.trim())
                .take_while(|l| !l.starts_with(']'))
                .collect();
            let flat = items.iter().all(|l| !l.starts_with(['{', '[']));
            if let Some(close) = lines.get(i + 1 + items.len()).filter(|_| flat) {
                out.push(format!("{line}{}{}", items.join(" "), close.trim()));
                i += items.len() + 2;
                continue;
            }
        }
        out.push(line.to_owned());
        i += 1;
    }
    println!("{}", out.join("\n"));
}
