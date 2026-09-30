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

Status: 0.1.0, not published. Targets Linux (Wayland) and macOS. The first
consumer is the Keeprs native client.

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
        let json = serde_json::to_string_pretty(&scene).expect("scenes serialize");
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

| Area | Supported |
| --- | --- |
| Shapes | Rectangle, diamond, ellipse; lines and arrows, straight or multi-point, with every Excalidraw arrowhead; drag any point, and Excalidraw's line editor to add and delete points |
| Text | Free text and labels inside shapes and on arrows, wrapped like Excalidraw, in Excalidraw's fonts; side handles re-wrap free text |
| Images | Rendered from `files[id].dataURL` with Excalidraw's crop and flips; insert, paste, copy between editors |
| Editing | Select, box select, move, Alt+drag duplicate, resize, rotate, delete, eraser, duplicate, flip, undo/redo, z-order, group, copy and paste styles, arrow binding with Excalidraw's highlight; lock and unlock (Ctrl+Shift+L), locked elements stay put |
| Style | Stroke and background color (swatches or hex), fill (hachure, cross-hatch, solid), stroke width and style, sloppiness, edges, opacity, arrowheads, font size, family and alignment |
| App | Excalidraw's clipboard format, pan and zoom (keyboard zoom and fit too), Excalidraw's keyboard shortcuts, dark mode with Excalidraw's canvas filter and a matching tool bar |
| Files | Excalidraw 0.18 JSON, lossless; older scenes load with Excalidraw's restore defaults; `Scene::version` tells when there is something to save |
| Export | SVG shaped like Excalidraw's `exportToSvg`, optionally with its fonts embedded |

Freedraw strokes made in Excalidraw are drawn (with perfect-freehand, exactly
like Excalidraw) and can be selected, moved, resized and erased; there is no
pen tool yet. Frames are drawn (outline, title, children cut off at the
frame) but not edited as frames: they cannot be clicked, drawn, or moved
with their children. Not supported: embeds, laser, elbow arrow routing
(drawn as polylines), editing an image's crop, snapping, collaboration. Elements of these types are
kept and saved untouched. Helvetica, Cascadia and Liberation Sans are not
bundled and render in the system sans or monospace font.

## Playground

```text
cargo run --release --example playground -- [scene.excalidraw]
```

Opens, edits and saves `.excalidraw` files, with Excalidraw's shortcuts. See
the top of `examples/playground.rs` for keys and the headless `--snapshot`
mode.

## How it is checked

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

Not decided yet. The bundled fonts keep their own licences, next to each
font under `assets/fonts/`: Excalifont, Virgil, Nunito and Lilita One are
under the SIL Open Font License 1.1, Comic Shanns under MIT.

[iced]: https://github.com/iced-rs/iced
[Excalidraw]: https://excalidraw.com
