//! Playground: open, edit and save `.excalidraw` files (or view an exported
//! Keeprs sketch memo).
//!
//! ```text
//! cargo run --release --example playground -- [file]
//! cargo run --release --example playground -- <file> --snapshot out.png [--dark] [--origin x,y]
//! ```
//!
//! Ctrl+O opens, Ctrl+S saves (Save As when the file is new or a Keeprs memo),
//! Ctrl+Shift+S saves as, Alt+Shift+D toggles dark mode. The canvas follows
//! Excalidraw's shortcuts: V selection, H hand, R rectangle, D diamond,
//! O ellipse, A arrow, L line (drag, or click point by point and finish with
//! Enter, Escape or a second click), T text (click a shape to label it;
//! double-click or Enter edits text; Escape or Ctrl+Enter finishes), Q keeps
//! the tool, Space or middle-drag
//! pans, Ctrl+scroll zooms, Delete, Ctrl+D, Ctrl+A, Ctrl+Z / Ctrl+Shift+Z,
//! arrow keys nudge, Ctrl+[ / Ctrl+] (with Shift: to back / front) reorder,
//! Ctrl+G / Ctrl+Shift+G group. Ctrl+C / Ctrl+X / Ctrl+V use Excalidraw's clipboard
//! format, so shapes paste between this and excalidraw.com; a copied image
//! pastes as an image. 9 inserts an image file.
//!
//! `--snapshot` renders headlessly at 100% zoom and writes
//! `out-<renderer>.png` (2x pixel density) instead of opening a window.
//! `--origin` sets the scene point at the top-left, e.g. to line up with an
//! Excalidraw SVG export (its first `translate`, negated).
use std::path::PathBuf;

use iced::keyboard::{self, key};
use iced::widget::{center, text};
use iced::{Element, Subscription, Task};
use roughdraft::scene::Scene;
use roughdraft::widget::{self, Appearance, EXCALIFONT, Sketch};
use serde_json::Value;

const USAGE: &str = "usage: playground [file.excalidraw | keeprs-memo.json] [--snapshot out.png] [--dark] [--origin x,y]";

/// Snapshot size in logical pixels.
const SNAPSHOT_SIZE: (f32, f32) = (1024.0, 768.0);

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let file = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from);
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
            .and_then(|(scene, _)| {
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
        return Ok(());
    }

    iced::application(
        move || Playground::open(file.clone(), appearance),
        Playground::update,
        Playground::view,
    )
    .title(Playground::title)
    .fonts([EXCALIFONT])
    .subscription(Playground::subscription)
    .run()
}

struct Playground {
    /// Where Ctrl+S writes; `None` for a new scene or a Keeprs memo.
    path: Option<PathBuf>,
    sketch: Sketch,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Sketch(widget::Message),
    ToggleAppearance,
    Open,
    Opened(Option<PathBuf>),
    Save { choose: bool },
    Saved(Result<PathBuf, String>),
    PickImage,
    ImagePicked(Option<Vec<u8>>),
}

impl Playground {
    fn open(file: Option<PathBuf>, appearance: Appearance) -> Self {
        let (scene, path, error) = match file.as_deref().map(load) {
            None => (Scene::default(), None, None),
            Some(Ok((scene, writable))) => (scene, file.filter(|_| writable), None),
            Some(Err(error)) => (Scene::default(), None, Some(error)),
        };
        let mut sketch = Sketch::new(scene);
        sketch.set_appearance(appearance);
        Self {
            path,
            sketch,
            error,
        }
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        format!(
            "{} - roughdraft playground",
            name.as_deref().unwrap_or("untitled")
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sketch(message) => return self.sketch.update(message).map(Message::Sketch),
            Message::ToggleAppearance => {
                self.sketch.set_appearance(match self.sketch.appearance() {
                    Appearance::Light => Appearance::Dark,
                    Appearance::Dark => Appearance::Light,
                })
            }
            Message::Open => return Task::perform(pick_file(), Message::Opened),
            Message::Opened(Some(path)) => *self = Self::open(Some(path), self.sketch.appearance()),
            Message::Opened(None) => {}
            Message::Save { choose } => {
                let json = serde_json::to_string_pretty(&self.sketch.scene().saved())
                    .expect("scenes serialize");
                let path = self.path.clone().filter(|_| !choose);
                return Task::perform(save_file(path, json), Message::Saved);
            }
            Message::Saved(Ok(path)) => {
                self.error = None;
                self.path = Some(path);
            }
            Message::Saved(Err(error)) => self.error = Some(error),
            Message::PickImage => return Task::perform(pick_image(), Message::ImagePicked),
            Message::ImagePicked(Some(bytes)) => {
                if let Err(error) = self.sketch.insert_image(&bytes) {
                    eprintln!("insert image: {error}");
                }
            }
            Message::ImagePicked(None) => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.error {
            // ponytail: errors replace the canvas; a toast when the toolbar lands (slice 4)
            Some(error) => center(text(error)).into(),
            None => self.sketch.view().map(Message::Sketch),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed {
                physical_key: key::Physical::Code(code),
                modifiers,
                ..
            } = event
            else {
                return None;
            };
            match code {
                key::Code::KeyD if modifiers.alt() && modifiers.shift() => {
                    Some(Message::ToggleAppearance)
                }
                key::Code::KeyO if modifiers.command() => Some(Message::Open),
                key::Code::KeyS if modifiers.command() => Some(Message::Save {
                    choose: modifiers.shift(),
                }),
                // Excalidraw's image tool key; text editing captures it first.
                key::Code::Digit9 if modifiers.is_empty() => Some(Message::PickImage),
                _ => None,
            }
        })
    }
}

/// Reads a scene file, unwrapping a Keeprs sketch memo
/// (`{"excalidraw": scene, "svg": ..., "savedInDarkMode": ...}`) if needed.
/// Also returns whether saving back to the same path keeps its format.
fn load(path: &std::path::Path) -> Result<(Scene, bool), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let json = std::fs::read_to_string(path).map_err(|e| fail(&e))?;
    let mut value: Value = serde_json::from_str(&json).map_err(|e| fail(&e))?;
    let memo = value.get("excalidraw").is_some();
    if let Some(inner) = value.get_mut("excalidraw").map(Value::take) {
        value = match inner {
            Value::String(nested) => serde_json::from_str(&nested).map_err(|e| fail(&e))?,
            other => other,
        };
    }
    let scene = serde_json::from_value(value).map_err(|e| fail(&e))?;
    Ok((scene, !memo))
}

async fn pick_file() -> Option<PathBuf> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Excalidraw", &["excalidraw", "json"])
        .pick_file()
        .await?;
    Some(file.path().to_owned())
}

async fn pick_image() -> Option<Vec<u8>> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp"])
        .pick_file()
        .await?;
    Some(file.read().await)
}

async fn save_file(path: Option<PathBuf>, json: String) -> Result<PathBuf, String> {
    let path = match path {
        Some(path) => path,
        None => rfd::AsyncFileDialog::new()
            .add_filter("Excalidraw", &["excalidraw"])
            .set_file_name("untitled.excalidraw")
            .save_file()
            .await
            .ok_or("save cancelled")?
            .path()
            .to_owned(),
    };
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn write_snapshot(sketch: &Sketch, png: &str) -> Result<(), String> {
    let settings = iced::Settings {
        fonts: vec![EXCALIFONT.into()],
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
