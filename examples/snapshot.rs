//! Renders a scene headlessly, the way the editor shows it, to
//! `<out>-<renderer>.png` at 2x pixel density.
//!
//! ```text
//! cargo run --release --example snapshot -- <file.excalidraw> <out.png> [--dark] [--origin x,y]
//! ```
//!
//! The content is centred, as when a file opens; `--origin` sets the scene
//! point at the top-left instead, e.g. to line up with an Excalidraw SVG
//! export (its first `translate`, negated).
use roughdraft::scene::Scene;
use roughdraft::widget::{Appearance, Sketch};

const USAGE: &str = "usage: snapshot <file.excalidraw> <out.png> [--dark] [--origin x,y]";

/// Snapshot size in logical pixels.
const SNAPSHOT_SIZE: (f32, f32) = (1024.0, 768.0);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (Some(file), Some(png)) = (positional.first(), positional.get(1)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let appearance = if args.iter().any(|a| a == "--dark") {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    // `--origin x,y` or `--origin=x,y` (the second form keeps a negative x
    // from reading as a flag).
    let origin = args
        .iter()
        .position(|a| a == "--origin")
        .and_then(|i| args.get(i + 1).map(String::as_str))
        .or_else(|| args.iter().find_map(|a| a.strip_prefix("--origin=")))
        .map(|xy| {
            let (x, y) = xy.split_once(',').expect("--origin takes x,y");
            [x.parse().expect("origin x"), y.parse().expect("origin y")]
        });
    let result = load(std::path::Path::new(file.as_str())).and_then(|scene| {
        let mut sketch = Sketch::new(scene);
        sketch.set_appearance(appearance);
        if let Some(origin) = origin {
            sketch.set_origin(origin);
        }
        write_snapshot(&sketch, png)
    });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// Reads a scene file.
fn load(path: &std::path::Path) -> Result<Scene, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let json = std::fs::read_to_string(path).map_err(|e| fail(&e))?;
    serde_json::from_str(&json).map_err(|e| fail(&e))
}

fn write_snapshot(sketch: &Sketch, png: &str) -> Result<(), String> {
    let settings = iced::Settings {
        fonts: roughdraft::fonts::ALL.map(Into::into).to_vec(),
        ..iced::Settings::default()
    };
    let mut simulator = iced_test::Simulator::with_size(settings, SNAPSHOT_SIZE, sketch.canvas());
    let snapshot = simulator
        .snapshot(&iced::Theme::Light)
        .map_err(|e| e.to_string())?;
    // `matches_image` writes the PNG when none exists yet.
    let stem = std::path::Path::new(png.trim_end_matches(".png"));
    let dir = stem
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let prefix = format!(
        "{}-",
        stem.file_name().unwrap_or_default().to_string_lossy()
    );
    for old in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = old.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(".png") {
            std::fs::remove_file(old.path()).map_err(|e| e.to_string())?;
        }
    }
    snapshot.matches_image(png).map_err(|e| e.to_string())?;
    Ok(())
}
