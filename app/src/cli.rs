//! Commands for agents and scripts (PLAN-002): `build` and `check`. They
//! never prompt, print a JSON result on stdout, and exit non-zero on
//! failure.
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
  roughdraft help                            show this

Each command prints JSON on stdout. build: {\"ok\": true, ...} or
{\"ok\": false, \"errors\": [{\"path\", \"message\"}]}. check: {\"ok\", \"problems\":
[{\"kind\", \"severity\", \"ids\", \"message\"}]}, ok when no errors. Exit codes:
0 done, 1 check found errors, 2 bad input.";

/// Runs the command in `args`; `None` when there is none, to open the
/// editor.
pub fn run(args: &[String]) -> Option<i32> {
    let (command, rest) = args.split_first()?;
    Some(match command.as_str() {
        "build" => build(rest),
        "check" => check(rest),
        "help" | "--help" | "-h" => {
            println!("{HELP}");
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
    let input = input.ok_or("missing the skeleton file (or - for stdin)")?;
    let output = output.ok_or("missing -o <out.excalidraw>")?;
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

fn print(value: Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("JSON values serialize")
    );
}
