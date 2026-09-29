//! Playground: view an `.excalidraw` file or an exported Keeprs sketch memo.
//!
//! ```text
//! cargo run --release --example playground -- <file>
//! cargo run --release --example playground -- <file> --snapshot out.png [--dark] [--origin x,y]
//! ```
//!
//! Drag or scroll to pan, Ctrl+scroll to zoom, Alt+Shift+D toggles dark mode.
//! `--snapshot` renders headlessly at 100% zoom and writes
//! `out-<renderer>.png` (2x pixel density) instead of opening a window.
//! `--origin` sets the scene point at the top-left, e.g. to line up with an
//! Excalidraw SVG export (its first `translate`, negated).
use iced::keyboard::{self, key};
use iced::widget::{center, text};
use iced::{Element, Subscription};
use roughdraft::scene::Scene;
use roughdraft::widget::{Appearance, EXCALIFONT, Viewer};
use serde_json::Value;

const USAGE: &str = "usage: playground <file.excalidraw | keeprs-memo.json> [--snapshot out.png] [--dark] [--origin x,y]";

/// Snapshot size in logical pixels.
const SNAPSHOT_SIZE: (f32, f32) = (1024.0, 768.0);

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = args.first().filter(|a| !a.starts_with("--")).cloned();
    let snapshot = args
        .iter()
        .position(|a| a == "--snapshot")
        .and_then(|i| args.get(i + 1));
    let appearance = if args.iter().any(|a| a == "--dark") {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    let origin = args
        .iter()
        .position(|a| a == "--origin")
        .and_then(|i| args.get(i + 1))
        .map(|xy| {
            let (x, y) = xy.split_once(',').expect("--origin takes x,y");
            [x.parse().expect("origin x"), y.parse().expect("origin y")]
        });

    if let Some(png) = snapshot {
        let result = file
            .ok_or_else(|| USAGE.to_owned())
            .and_then(|f| load(&f))
            .map(|scene| {
                let mut viewer = Viewer::new(&scene);
                viewer.set_appearance(appearance);
                if let Some(origin) = origin {
                    viewer.set_origin(origin);
                }
                write_snapshot(&viewer, png)
            });
        if let Err(error) = result.and_then(|written| written) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    iced::application(
        move || Playground::open(file.as_deref(), appearance),
        Playground::update,
        Playground::view,
    )
    .title("roughdraft playground")
    .fonts([EXCALIFONT])
    .subscription(Playground::subscription)
    .run()
}

struct Playground {
    viewer: Result<Viewer, String>,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    ToggleAppearance,
}

impl Playground {
    fn open(file: Option<&str>, appearance: Appearance) -> Self {
        let viewer = file
            .ok_or_else(|| USAGE.to_owned())
            .and_then(load)
            .map(|scene| {
                let mut viewer = Viewer::new(&scene);
                viewer.set_appearance(appearance);
                viewer
            });
        Self { viewer }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::ToggleAppearance => {
                if let Ok(viewer) = &mut self.viewer {
                    viewer.set_appearance(match viewer.appearance() {
                        Appearance::Light => Appearance::Dark,
                        Appearance::Dark => Appearance::Light,
                    });
                }
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.viewer {
            Ok(viewer) => viewer.view(),
            Err(error) => center(text(error)).into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                physical_key: key::Physical::Code(key::Code::KeyD),
                modifiers,
                ..
            } if modifiers.alt() && modifiers.shift() => Some(Message::ToggleAppearance),
            _ => None,
        })
    }
}

/// Reads a scene file, unwrapping a Keeprs sketch memo
/// (`{"excalidraw": scene, "svg": ..., "savedInDarkMode": ...}`) if needed.
fn load(path: &str) -> Result<Scene, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut value: Value = serde_json::from_str(&json).map_err(|e| format!("{path}: {e}"))?;
    if let Some(inner) = value.get_mut("excalidraw").map(Value::take) {
        value = match inner {
            Value::String(nested) => {
                serde_json::from_str(&nested).map_err(|e| format!("{path}: {e}"))?
            }
            other => other,
        };
    }
    serde_json::from_value(value).map_err(|e| format!("{path}: {e}"))
}

fn write_snapshot(viewer: &Viewer, png: &str) -> Result<(), String> {
    let settings = iced::Settings {
        fonts: vec![EXCALIFONT.into()],
        ..iced::Settings::default()
    };
    let mut simulator =
        iced_test::Simulator::with_size(settings, SNAPSHOT_SIZE, viewer.view::<()>());
    let snapshot = simulator
        .snapshot(&iced::Theme::Light)
        .map_err(|e| e.to_string())?;
    // `matches_image` writes the PNG when none exists yet.
    let stem = png.trim_end_matches(".png");
    for old in std::fs::read_dir(".").into_iter().flatten().flatten() {
        let name = old.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{stem}-")) && name.ends_with(".png") {
            std::fs::remove_file(old.path()).map_err(|e| e.to_string())?;
        }
    }
    snapshot.matches_image(png).map_err(|e| e.to_string())?;
    Ok(())
}
