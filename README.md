# roughdraft

A native, Excalidraw-compatible sketch canvas for [iced]. It reads, renders,
edits and writes [Excalidraw] 0.18 scene JSON, so a drawing made here opens
in excalidraw.com unchanged, and the other way round.

- Same `seed`, same wobble: shapes are drawn with a seed-exact port of
  rough.js 4.6.4, the version Excalidraw 0.18 uses.
- Text in Excalidraw's own fonts (Excalifont, Virgil, Nunito, Lilita One,
  Comic Shanns, all bundled), measured and wrapped like Excalidraw does it.
- Lossless: unknown fields and element types round-trip byte for byte.
- An iced-free core (`scene`, `render`, `edit`, `svg`), so scenes can be
  edited, tested and exported to SVG without a window.

Status: 0.1.0, not published. Targets Linux (Wayland) and macOS. It is both
a library for iced apps and a standalone editor (the `roughdraft` app).

## Embedding

Keep a `Sketch` in your state, show its view, and pass its messages back:

```rust
use roughdraft::scene::Scene;
use roughdraft::svg::{self, SvgOptions};
use roughdraft::widget::{self, Sketch};

struct Notes {
    sketch: Sketch,
}

#[derive(Debug, Clone)]
enum Message {
    Sketch(widget::Message),
}

impl Notes {
    fn open(json: &str) -> serde_json::Result<Self> {
        let scene: Scene = serde_json::from_str(json)?;
        Ok(Self { sketch: Sketch::new(scene) })
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Sketch(message) => self.sketch.update(message).map(Message::Sketch),
        }
    }

    fn view(&self) -> iced::Element<'_, Message> {
        // The canvas with Excalidraw's tool bar and style panel over it;
        // `canvas()` is the bare canvas.
        self.sketch.view().map(Message::Sketch)
    }

    /// Returns what to store: JSON that Excalidraw opens, and an SVG preview.
    fn save(&self) -> (String, String) {
        // `saved` drops deleted elements and unused image files, like
        // Excalidraw's own save.
        let scene = self.sketch.scene().saved();
        // Two-space indents and JavaScript's number spelling, like
        // Excalidraw's own files.
        let json = scene.to_json();
        (json, svg::export(&scene, &SvgOptions::default()))
    }
}

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let notes = Notes::open(r#"{"type": "excalidraw", "version": 2, "elements": []}"#)?;
let (json, preview) = notes.save();
assert!(json.contains(r#""type": "excalidraw""#));
assert!(preview.starts_with("<svg"));
# Ok(())
# }
```

Run it with `iced::application(..)` as usual. To autosave, keep
`sketch.scene().version()` from the last save and save again when it
differs; it changes on every edit and returns to the saved value when the
edits are undone. Images the scene stores
elsewhere (not as data URLs) are supplied with `Sketch::set_image`.
`Sketch::insert_image` adds an image file, and Ctrl+V pastes a copied image.

## What it edits

| Area    | Supported                                                                                                                                                                                                                                                                                                       |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shapes  | Rectangle, diamond, ellipse; lines and arrows, straight or multi-point, with every Excalidraw arrowhead; drag any point, and Excalidraw's line editor to add and delete points                                                                                                                                  |
| Text    | Free text and labels inside shapes and on arrows, wrapped like Excalidraw, in Excalidraw's fonts; side handles re-wrap free text                                                                                                                                                                                |
| Images  | Rendered from `files[id].dataURL` with Excalidraw's crop and flips; insert, paste, copy between editors                                                                                                                                                                                                         |
| Editing | Select, box select, move, Alt+drag duplicate, resize, rotate, delete, eraser, duplicate, flip, undo/redo, z-order, group, copy and paste styles, arrow binding with Excalidraw's highlight; lock and unlock (Ctrl+Shift+L), locked elements stay put                                                            |
| Style   | Stroke and background color (swatches or hex), fill (hachure, cross-hatch, solid), stroke width and style, sloppiness, edges, opacity, arrowheads, font size, family and alignment                                                                                                                              |
| App     | A main menu in the top-left corner (the host's open, save and export actions come back as `Message::request`; dark/light built in), Excalidraw's clipboard format, pan and zoom (keyboard zoom and fit too), Excalidraw's keyboard shortcuts, dark mode with Excalidraw's canvas filter and a matching tool bar |
| Files   | Excalidraw 0.18 JSON, lossless; older scenes load with Excalidraw's restore defaults; `Scene::version` tells when there is something to save                                                                                                                                                                    |
| Export  | SVG shaped like Excalidraw's `exportToSvg`, optionally with its fonts embedded                                                                                                                                                                                                                                  |

Freedraw strokes made in Excalidraw are drawn (with perfect-freehand, exactly
like Excalidraw) and can be selected, moved, resized and erased; there is no
pen tool yet. Frames are drawn (outline, title, children cut off at the
frame) and are selected by their outline or title; moving, duplicating,
copying, flipping, locking or erasing one takes its children along, and
deleting one keeps them. The frame tool (F) takes in what lies wholly
inside the new frame, and dragged elements join the frame they are
dropped on or leave the one they were dragged out of. Not supported: embeds, laser, elbow arrow routing
(drawn as polylines), editing an image's crop, snapping, collaboration. Elements of these types are
kept and saved untouched. Helvetica, Cascadia and Liberation Sans are not
bundled and render in the system sans or monospace font.

## The app

Download it from [Releases](https://github.com/vihu/roughdraft/releases):

- Linux (x86_64 and arm64), either of:
  - `roughdraft-<version>-<arch>.flatpak`: run
    `flatpak install --user roughdraft-<version>-<arch>.flatpak` (it fetches
    the runtime from Flathub), then start roughdraft from the app menu.
  - `roughdraft-<version>-<arch>.AppImage`: make it executable
    (`chmod +x`) and run it. Needs glibc 2.35 or newer (Ubuntu 22.04,
    Debian 12, Fedora 36 and later).
- macOS 11 or newer (Apple silicon and Intel):
  `roughdraft-<version>-macos-universal.zip`. Unzip it and move
  `roughdraft.app` to Applications. The app is not notarized, so macOS blocks
  the first launch: allow it in System Settings > Privacy & Security > Open
  Anyway, or run `xattr -dr com.apple.quarantine /Applications/roughdraft.app`.

Or run it from source:

```text
cargo run --release -p roughdraft-app -- [scene.excalidraw] [--dark]
```

`roughdraft`, the desktop editor in `app/`: opens, edits, saves and exports
`.excalidraw` files, with Excalidraw's shortcuts and a main menu. See the top
of `app/src/main.rs` for keys. `cargo run --release --example snapshot`
renders a scene headlessly to a PNG.

## How it is checked

CI runs all of this on Linux and macOS for every push to `main` and every pull request.

- `tests/rough_parity.rs`: 165 rough.js cases, generated by rough.js itself.
- `tests/scene_round_trip.rs`: every fixture reads and writes back byte for
  byte.
- `tests/svg_parity.rs`: every mark of our render and of our SVG export
  matches an SVG made by real Excalidraw 0.18.1 (through `excalidraw-image`),
  including scenes drawn with this editor.
- Behaviour tests for the editor in `src/edit/tests/`, and headless widget
  tests driven through iced's simulator in `tests/sketch_widget.rs`.
- `tests/editor_fuzz.rs`: seeded random editing sessions that must never
  panic and must keep every reference in the scene valid.
- `cargo run --release --example bench` measures frame times on a
  500-element scene.

## Licence

MIT, see `LICENSE`. Ports of Excalidraw and perfect-freehand code keep
their MIT notices (`THIRD-PARTY-NOTICES.md`). The bundled fonts keep their
own licences, next to each font under `assets/fonts/`: Excalifont, Virgil,
Nunito and Lilita One are under the SIL Open Font License 1.1, Comic
Shanns under MIT.

[iced]: https://github.com/iced-rs/iced
[Excalidraw]: https://excalidraw.com
