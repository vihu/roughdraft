//! Playground: open, edit and save `.excalidraw` files (or view an exported
//! Keeprs sketch memo).
//!
//! ```text
//! cargo run --release --example playground -- [file]
//! cargo run --release --example playground -- <file> --snapshot out.png [--dark] [--origin x,y]
//! ```
//!
//! Keys follow Excalidraw:
//!
//! - Files: the menu in the top-left corner (open, save, save as, export
//!   SVG, insert image, dark/light), or Ctrl+O open, Ctrl+S save (asks where
//!   for new files and Keeprs memos), Ctrl+Shift+S save as, 9 insert an
//!   image file. The title shows `*` while there are unsaved changes.
//! - Tools: V or 1 select, H hand, R or 2 rectangle, D or 3 diamond, O or 4
//!   ellipse, A or 5 arrow, L or 6 line, T or 8 text, E or 0 eraser (drag
//!   across elements; Alt un-marks), F frame (takes in what is wholly
//!   inside); Q keeps the tool.
//!   Lines and arrows: drag, or click point by point and finish with Enter,
//!   Escape or a click on the last point; drag any point of a selected one;
//!   Ctrl+Enter or Ctrl+double-click opens the line editor (click and
//!   Shift-click points, drag them, drag a segment middle to add one,
//!   Delete removes them, Escape leaves). Text: click a shape to label it;
//!   double-click or Enter edits; Escape or Ctrl+Enter finishes.
//! - Edit: Delete, Ctrl+D duplicate, Alt+drag duplicate, Ctrl+A select all,
//!   Ctrl+Z / Ctrl+Shift+Z undo and redo, arrow keys nudge (Shift: 5),
//!   Ctrl+[ / Ctrl+] one step back or forward (with Shift: to back or
//!   front), Ctrl+G / Ctrl+Shift+G group and ungroup, Ctrl+Shift+< / >
//!   text size, Ctrl+Alt+C / Ctrl+Alt+V copy and paste styles, Shift+H /
//!   Shift+V flip, Ctrl+Shift+L lock or unlock (nothing selected: unlock
//!   all).
//! - Clipboard: Ctrl+C / Ctrl+X / Ctrl+V in Excalidraw's format, so shapes
//!   paste between this and excalidraw.com; a copied image pastes as an
//!   image.
//! - View: Space-drag, middle-drag or the scroll wheel pan; Ctrl+scroll
//!   zooms; Ctrl+= / Ctrl+- / Ctrl+0 step and reset the zoom; Shift+1 fits
//!   everything, Shift+2 / Shift+3 the selection; Alt+Shift+D dark mode.
//!
//! `--snapshot` renders headlessly at 100% zoom and writes
//! `out-<renderer>.png` (2x pixel density) instead of opening a window.
//! The content is centred, as when a file opens; `--origin` sets the scene
//! point at the top-left instead, e.g. to line up with an Excalidraw SVG
//! export (its first `translate`, negated).
use std::path::PathBuf;

use iced::keyboard::{self, key};
use iced::widget::{container, stack, text};
use iced::{Element, Subscription, Task};
use roughdraft::scene::Scene;
use roughdraft::svg::{self, SvgOptions};
use roughdraft::widget::{self, Appearance, Request, Sketch};
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
    .fonts(roughdraft::fonts::ALL)
    .subscription(Playground::subscription)
    .run()
}

struct Playground {
    /// Where Ctrl+S writes; `None` for a new scene or a Keeprs memo.
    path: Option<PathBuf>,
    sketch: Sketch,
    error: Option<String>,
    /// `Scene::version` when last opened or saved.
    saved: u32,
}

#[derive(Debug, Clone)]
enum Message {
    Sketch(widget::Message),
    ToggleAppearance,
    Open,
    Opened(Option<PathBuf>),
    Save {
        choose: bool,
    },
    /// Where it was written (`None` when the dialog was cancelled), and the
    /// scene version that was written.
    Saved(Result<Option<PathBuf>, String>, u32),
    PickImage,
    ImagePicked(Option<Vec<u8>>),
    ExportSvg,
    /// Where the SVG went (`None` when the dialog was cancelled).
    Exported(Result<Option<PathBuf>, String>),
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
        sketch.set_menu(vec![
            Request::Open,
            Request::Save,
            Request::SaveAs,
            Request::ExportSvg,
            Request::InsertImage,
        ]);
        Self {
            path,
            saved: sketch.scene().version(),
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
        let unsaved = if self.sketch.scene().version() == self.saved {
            ""
        } else {
            "*"
        };
        format!(
            "{}{unsaved} - roughdraft playground",
            name.as_deref().unwrap_or("untitled")
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sketch(message) => {
                // A main menu item: the playground carries it out.
                let request = message.request().map(|request| match request {
                    Request::Open => Message::Open,
                    Request::Save => Message::Save { choose: false },
                    Request::SaveAs => Message::Save { choose: true },
                    Request::ExportSvg => Message::ExportSvg,
                    Request::InsertImage => Message::PickImage,
                });
                let task = self.sketch.update(message).map(Message::Sketch);
                let follow = request.map_or_else(Task::none, |request| self.update(request));
                return Task::batch([task, follow]);
            }
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
                let json = self.sketch.scene().saved().to_json();
                let version = self.sketch.scene().version();
                let path = self.path.clone().filter(|_| !choose);
                return Task::perform(save_file(path, json), move |result| {
                    Message::Saved(result, version)
                });
            }
            Message::Saved(Ok(Some(path)), version) => {
                self.error = None;
                self.path = Some(path);
                self.saved = version;
            }
            Message::Saved(Ok(None), _) => {}
            Message::Saved(Err(error), _) => self.error = Some(error),
            Message::PickImage => return Task::perform(pick_image(), Message::ImagePicked),
            Message::ImagePicked(Some(bytes)) => {
                if let Err(error) = self.sketch.insert_image(&bytes) {
                    eprintln!("insert image: {error}");
                }
            }
            Message::ImagePicked(None) => {}
            Message::ExportSvg => {
                let image = svg::export(&self.sketch.scene().saved(), &SvgOptions::default());
                let name = self
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map_or("untitled".into(), |s| s.to_string_lossy().into_owned());
                return Task::perform(export_svg(format!("{name}.svg"), image), Message::Exported);
            }
            Message::Exported(Err(error)) => self.error = Some(error),
            Message::Exported(Ok(_)) => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let sketch = self.sketch.view().map(Message::Sketch);
        match &self.error {
            // The last open or save error, over the bottom of the canvas
            // until the next successful save.
            Some(error) => stack![
                sketch,
                container(
                    text(error)
                        .size(14)
                        .color(iced::Color::from_rgb8(0xe0, 0x31, 0x31))
                )
                .align_bottom(iced::Length::Fill)
                .padding(16),
            ]
            .into(),
            None => sketch,
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

async fn save_file(path: Option<PathBuf>, json: String) -> Result<Option<PathBuf>, String> {
    let path = match path {
        Some(path) => path,
        None => {
            let dialog = rfd::AsyncFileDialog::new()
                .add_filter("Excalidraw", &["excalidraw"])
                .set_file_name("untitled.excalidraw");
            let Some(file) = dialog.save_file().await else {
                return Ok(None);
            };
            file.path().to_owned()
        }
    };
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

async fn export_svg(name: String, image: String) -> Result<Option<PathBuf>, String> {
    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("SVG", &["svg"])
        .set_file_name(name);
    let Some(file) = dialog.save_file().await else {
        return Ok(None);
    };
    let path = file.path().to_owned();
    std::fs::write(&path, image).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
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
