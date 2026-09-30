//! Random editing sessions: pointer gestures, commands, text, style changes
//! and clipboard round trips in any order must never panic, and must leave a
//! scene that saves, reloads unchanged, keeps its references and renders.
//! Seeded, so a failure replays from the seed and step in its message.
use std::collections::HashSet;

use roughdraft::edit::{Command, Editor, Modifiers, Order, Pointer, StyleChange, Tool};
use roughdraft::scene::{ArrowEnd, Arrowhead, FillStyle, Kind, Scene, StrokeStyle, TextAlign};

/// Sessions, steps per session, and how often the scene is also saved,
/// reloaded and rendered (references are checked every step).
const SEEDS: u64 = 12;
const STEPS: usize = 300;
const FULL_CHECK_EVERY: usize = 20;

/// `ROUGHDRAFT_FUZZ_SEEDS=<n>` runs more sessions, e.g. with `--release`.
#[test]
fn random_sessions_keep_the_scene_valid() {
    let seeds = std::env::var("ROUGHDRAFT_FUZZ_SEEDS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(SEEDS);
    for seed in 0..seeds {
        session(seed);
    }
}

fn session(seed: u64) {
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/scenes/l2-created.excalidraw"
    ))
    .unwrap();
    let mut editor = Editor::new(serde_json::from_str::<Scene>(&fixture).unwrap());
    let mut rng = Rng(seed);
    let mut clipboard: Option<String> = None;
    for step in 0..STEPS {
        check(&editor, seed, step, step % FULL_CHECK_EVERY == 0);
        let modifiers = Modifiers {
            shift: rng.below(5) == 0,
            alt: rng.below(8) == 0,
            command: false,
        };
        match rng.below(22) {
            // Drag a handle of the selection: corners, knob, points, midpoint.
            20 => {
                let Some(handles) = editor.handles() else {
                    continue;
                };
                let targets: Vec<[f64; 2]> = handles
                    .handles
                    .iter()
                    .map(|(_, p)| *p)
                    .chain(handles.points.iter().copied())
                    .chain(handles.midpoints.iter().map(|(_, p)| *p))
                    .collect();
                if targets.is_empty() {
                    continue;
                }
                let from = rng.pick(&targets);
                editor.pointer(Pointer::Down, from, modifiers);
                for _ in 0..1 + rng.below(3) {
                    editor.pointer(Pointer::Move, rng.point(), modifiers);
                }
                editor.pointer(Pointer::Up, rng.point(), modifiers);
            }
            21 => {
                let url = "data:image/png;base64,iVBORw0KGgo=".to_owned();
                editor.insert_image(url, "image/png", [40.0, 30.0], rng.point());
            }
            0..=7 => {
                editor.pointer(Pointer::Down, rng.point(), modifiers);
                for _ in 0..rng.below(4) {
                    editor.pointer(Pointer::Move, rng.point(), modifiers);
                }
                editor.pointer(Pointer::Up, rng.point(), modifiers);
            }
            8 => editor.pointer(Pointer::Hover, rng.point(), modifiers),
            9 => editor.command(Command::Tool(rng.pick(&[
                Tool::Selection,
                Tool::Rectangle,
                Tool::Diamond,
                Tool::Ellipse,
                Tool::Arrow,
                Tool::Line,
                Tool::Text,
                Tool::Hand,
            ]))),
            10 => editor.command(rng.pick(&[
                Command::EditLine,
                Command::EditLine,
                Command::ToggleLock,
                Command::Escape,
                Command::Finish,
                Command::Delete,
                Command::Duplicate,
                Command::SelectAll,
                Command::Group,
                Command::Ungroup,
                Command::LargerFont,
                Command::SmallerFont,
            ])),
            11 => editor.command(rng.pick(&[
                Command::Nudge([1.0, 0.0]),
                Command::Nudge([0.0, -5.0]),
                Command::Reorder(Order::Forward),
                Command::Reorder(Order::Backward),
                Command::Reorder(Order::ToFront),
                Command::Reorder(Order::ToBack),
            ])),
            12 => editor.double_click(rng.point()),
            13 => editor.set_text(rng.pick(&[
                "",
                "a",
                "hello world",
                "two\nlines",
                "long words wrap around",
            ])),
            14 => editor.finish_text(),
            15 => editor.apply_style(rng.style()),
            16 => {
                clipboard = if rng.below(2) == 0 {
                    editor.copy()
                } else {
                    editor.cut()
                }
            }
            17 => {
                if let Some(json) = &clipboard {
                    editor.paste(json, rng.point());
                }
            }
            18 => editor.command(rng.pick(&[Command::Undo, Command::Redo])),
            _ => editor.set_zoom(rng.pick(&[0.25, 0.5, 1.0, 2.0, 4.0])),
        }
    }
    check(&editor, seed, STEPS, true);
}

/// Cross-checks the scene's references; with `full`, also saves, reloads
/// and renders it.
fn check(editor: &Editor, seed: u64, step: usize, full: bool) {
    let at = format!("seed {seed}, step {step}");
    let saved = editor.scene().saved();
    if full {
        let json = serde_json::to_string(&saved).unwrap();
        let back: Scene =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("{at}: reload: {e}"));
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            json,
            "{at}: reload changed the scene"
        );
    }

    let live: HashSet<&str> = saved.elements.iter().map(|e| e.base.id.as_str()).collect();
    assert_eq!(live.len(), saved.elements.len(), "{at}: duplicate ids");
    for element in &saved.elements {
        let id = &element.base.id;
        if let Kind::Text(text) = &element.kind
            && let Some(container) = &text.container_id
        {
            assert!(
                live.contains(container.as_str()),
                "{at}: label {id} lost its container"
            );
        }
        for end in [ArrowEnd::Start, ArrowEnd::End] {
            if let Some(binding) = element.binding(end) {
                assert!(
                    live.contains(binding.element_id.as_str()),
                    "{at}: {id} bound to a missing shape"
                );
            }
        }
        if let Some(file) = element.file_id() {
            assert!(
                saved.file_data_url(file).is_some(),
                "{at}: image {id} lost its file"
            );
        }
        for bound in element.base.bound_elements.iter().flatten() {
            assert!(
                live.contains(bound.id.as_str()),
                "{at}: {id} lists missing {}",
                bound.id
            );
        }
        assert!(
            [
                element.base.x,
                element.base.y,
                element.base.width,
                element.base.height
            ]
            .iter()
            .all(|v| v.is_finite()),
            "{at}: {id} has a non-finite box"
        );
    }
    if full {
        for element in &saved.elements {
            let _ = roughdraft::render::render_element(element, "#ffffff");
        }
        let _ = roughdraft::svg::export(&saved, &roughdraft::svg::SvgOptions::default());
    }
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize].clone()
    }

    /// A point around the fixture's content (about 0..740 x 0..260).
    fn point(&mut self) -> [f64; 2] {
        [self.below(900) as f64 - 80.0, self.below(420) as f64 - 80.0]
    }

    fn style(&mut self) -> StyleChange {
        match self.below(12) {
            0 => StyleChange::StrokeColor(self.pick(&["#1e1e1e", "#e03131", "#abc"]).into()),
            1 => StyleChange::BackgroundColor(self.pick(&["transparent", "#ffec99"]).into()),
            2 => StyleChange::FillStyle(self.pick(&[
                FillStyle::Hachure,
                FillStyle::CrossHatch,
                FillStyle::Solid,
            ])),
            3 => StyleChange::StrokeWidth(self.pick(&[1.0, 2.0, 4.0])),
            4 => StyleChange::StrokeStyle(self.pick(&[
                StrokeStyle::Solid,
                StrokeStyle::Dashed,
                StrokeStyle::Dotted,
            ])),
            5 => StyleChange::Roughness(self.pick(&[0.0, 1.0, 2.0])),
            6 => StyleChange::Opacity(self.pick(&[10.0, 60.0, 100.0])),
            7 => StyleChange::RoundEdges(self.below(2) == 0),
            8 => StyleChange::EndArrowhead(self.pick(&[
                None,
                Some(Arrowhead::Arrow),
                Some(Arrowhead::Triangle),
            ])),
            9 => StyleChange::FontSize(self.pick(&[16.0, 20.0, 36.0])),
            10 => StyleChange::FontFamily(self.pick(&[1, 5, 6, 8])),
            _ => StyleChange::TextAlign(self.pick(&[
                TextAlign::Left,
                TextAlign::Center,
                TextAlign::Right,
            ])),
        }
    }
}
