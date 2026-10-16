//! Random input through the whole editor until something panics: the stress
//! run for crashes that close the app's window.
//!
//! ```text
//! cargo run --release --example stress -- [seeds] [frames] [wgpu|tiny-skia]
//! ```
//!
//! Each session opens a fixture scene in `Sketch::view` (canvas, tool bar,
//! style panel, menu, text overlay) and feeds it seeded clicks, drags, wheel
//! pans and zooms, keys, typing, window resizes and far pans, frame by frame
//! the way the app's event loop does: events to the `UserInterface`, its
//! messages to `Sketch::update`, the tasks' widget operations (text focus)
//! to the rebuilt interface, then a full draw. tiny-skia also rasterizes
//! every frame; wgpu stops at tessellation, as its headless screenshot
//! panics once geometry changes (see `bench.rs`). Clipboard tasks get no
//! answer, so paste does nothing here; window and compositor faults are out
//! of reach.
//!
//! A panic or a broken invariant (zoom outside its limits, an element at a
//! non-finite place) ends that session with its seed and frame; the run goes
//! on, then lists the panics by place and exits 1 if there were any.
//! `ROUGHDRAFT_STRESS_ONLY=<seed>` replays one session; `RUST_BACKTRACE=1`
//! prints a backtrace for each new place. A hard crash (abort, segfault)
//! ends the run: replay the last `seed` it printed.
use std::backtrace::{Backtrace, BacktraceStatus};
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;

use iced::advanced::renderer::Headless;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::mouse::{self, Button, ScrollDelta};
use iced::{Event, Point, Size};
use roughdraft::scene::Scene;
use roughdraft::widget::Sketch;

#[path = "../tests/common/window.rs"]
mod window;

use window::Window;

/// Window sizes a resize picks from; the usual ones more often.
const SIZES: [(f32, f32); 9] = [
    (1.0, 1.0),
    (64.0, 48.0),
    (320.0, 240.0),
    (800.0, 600.0),
    (1280.0, 800.0),
    (1280.0, 800.0),
    (1920.0, 1080.0),
    (1920.0, 1080.0),
    (3840.0, 2160.0),
];

/// Keys with a shortcut (tools, undo, group, zoom, grid, order, fonts).
const CHARACTERS: [(&str, Code); 28] = [
    ("v", Code::KeyV),
    ("h", Code::KeyH),
    ("r", Code::KeyR),
    ("d", Code::KeyD),
    ("o", Code::KeyO),
    ("a", Code::KeyA),
    ("l", Code::KeyL),
    ("p", Code::KeyP),
    ("x", Code::KeyX),
    ("t", Code::KeyT),
    ("e", Code::KeyE),
    ("f", Code::KeyF),
    ("q", Code::KeyQ),
    ("z", Code::KeyZ),
    ("y", Code::KeyY),
    ("g", Code::KeyG),
    ("c", Code::KeyC),
    ("1", Code::Digit1),
    ("2", Code::Digit2),
    ("3", Code::Digit3),
    ("0", Code::Digit0),
    ("=", Code::Equal),
    ("-", Code::Minus),
    ("'", Code::Quote),
    ("[", Code::BracketLeft),
    ("]", Code::BracketRight),
    (",", Code::Comma),
    (".", Code::Period),
];

const NAMED: [(Named, Code); 14] = [
    (Named::Delete, Code::Delete),
    (Named::Backspace, Code::Backspace),
    (Named::Escape, Code::Escape),
    (Named::Enter, Code::Enter),
    (Named::ArrowLeft, Code::ArrowLeft),
    (Named::ArrowRight, Code::ArrowRight),
    (Named::ArrowUp, Code::ArrowUp),
    (Named::ArrowDown, Code::ArrowDown),
    (Named::Space, Code::Space),
    (Named::Tab, Code::Tab),
    (Named::PageUp, Code::PageUp),
    (Named::PageDown, Code::PageDown),
    (Named::Home, Code::Home),
    (Named::End, Code::End),
];

/// Typed text: accents, combining marks, CJK, emoji, right-to-left, a
/// zero-width space.
const TYPED: [&str; 10] = [
    "a", "W", " ", "é", "e\u{301}", "漢", "😀", "ש", "\u{200b}", "\t",
];

/// The place, message and (with `RUST_BACKTRACE`) backtrace of the last
/// panic.
static PANIC: Mutex<Option<(String, String, Option<String>)>> = Mutex::new(None);

fn main() {
    let mut args = std::env::args().skip(1);
    let seeds: u64 = args.next().and_then(|n| n.parse().ok()).unwrap_or(100);
    let frames: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(500);
    let backend = args.next().unwrap_or_else(|| "wgpu".into());
    let only: Option<u64> = std::env::var("ROUGHDRAFT_STRESS_ONLY")
        .ok()
        .and_then(|n| n.parse().ok());
    let scenes = fixtures();

    panic::set_hook(Box::new(|info| {
        let place = info.location().map(ToString::to_string).unwrap_or_default();
        let message = info.payload_as_str().unwrap_or_default().to_owned();
        let backtrace = Backtrace::capture();
        let backtrace =
            (backtrace.status() == BacktraceStatus::Captured).then(|| backtrace.to_string());
        if let Ok(mut last) = PANIC.lock() {
            *last = Some((place, message, backtrace));
        }
    }));

    let mut renderer = None;
    // Panics by place: the first message, and every seed and frame.
    let mut places: BTreeMap<String, (String, Vec<(u64, usize)>)> = BTreeMap::new();
    let mut sessions = 0;
    let mut name = String::new();
    for seed in only.map_or(0..seeds, |seed| seed..seed + 1) {
        eprint!("\rseed {seed}");
        sessions += 1;
        let headless = renderer.get_or_insert_with(|| window::renderer(Some(&backend)));
        name = headless.name();
        let present = name != "wgpu";
        let scene = &scenes[seed as usize % scenes.len()];
        let mut frame = 0;
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            session(scene, seed, frames, headless, present, &mut frame)
        }));
        if outcome.is_ok() {
            continue;
        }
        let (place, message, backtrace) = PANIC
            .lock()
            .ok()
            .and_then(|mut p| p.take())
            .unwrap_or_default();
        eprintln!("\rseed {seed} frame {frame}: {message} ({place})");
        let (_, hits) = places.entry(place).or_insert_with(|| {
            if let Some(backtrace) = backtrace {
                eprintln!("{backtrace}");
            }
            (message, Vec::new())
        });
        hits.push((seed, frame));
        // The renderer may be mid-frame: the next session gets a new one.
        renderer = None;
    }

    let failed: usize = places.values().map(|(_, hits)| hits.len()).sum();
    eprintln!("\r{sessions} sessions of {frames} frames on {name}: {failed} panicked");
    for (place, (message, hits)) in &places {
        let (seed, frame) = hits[0];
        eprintln!(
            "  {}x {place}: {message} (first: seed {seed} frame {frame})",
            hits.len()
        );
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

/// The hand-made scenes, in name order.
fn fixtures() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scenes");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "excalidraw"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect()
}

fn session(
    json: &str,
    seed: u64,
    frames: usize,
    renderer: &mut iced::Renderer,
    present: bool,
    frame: &mut usize,
) {
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    let mut rng = Rng(seed);
    let mut window = Window::new(Size::new(1280.0, 800.0));
    while *frame < frames {
        for events in gesture(&mut rng, &mut window) {
            window.frame(&mut sketch, renderer, &events, present);
            check(&sketch);
            *frame += 1;
        }
    }
}

/// What a frame must leave behind.
fn check(sketch: &Sketch) {
    let zoom = sketch.zoom();
    assert!((0.1..=30.0).contains(&zoom), "zoom {zoom} outside 0.1..=30");
    for element in &sketch.scene().elements {
        let b = &element.base;
        assert!(
            [b.x, b.y, b.width, b.height, b.angle]
                .iter()
                .all(|v| v.is_finite()),
            "element {} at a non-finite place",
            b.id
        );
    }
}

/// One gesture: the events of each frame it spans.
fn gesture(rng: &mut Rng, window: &mut Window) -> Vec<Vec<Event>> {
    let size = window.size;
    // Any gesture may start with other modifier keys held.
    let mut first: Vec<Event> = Vec::new();
    if rng.below(3) == 0 {
        window.modifiers = rng.modifiers();
        first.push(Event::Keyboard(keyboard::Event::ModifiersChanged(
            window.modifiers,
        )));
    }
    let modifiers = window.modifiers;
    let mut frames = match rng.below(20) {
        // A click, sometimes a double-click.
        0..=3 => {
            let at = rng.place(size);
            let clicks = 1 + usize::from(rng.below(4) == 0);
            let mut frame = vec![moved(at)];
            for _ in 0..clicks {
                frame.extend([press(Button::Left), release(Button::Left)]);
            }
            vec![frame]
        }
        // A drag over several frames, up to 8 moves each (a fast mouse);
        // the middle button pans.
        4..=7 => {
            let button = rng.pick(&[Button::Left, Button::Left, Button::Middle, Button::Right]);
            let mut at = rng.place(size);
            let mut frames = vec![vec![moved(at), press(button)]];
            for _ in 0..=rng.below(30) {
                let moves = (0..=rng.below(8))
                    .map(|_| {
                        at = rng.near(at, size);
                        moved(at)
                    })
                    .collect();
                frames.push(moves);
            }
            frames.push(vec![release(button)]);
            frames
        }
        // Hovering.
        8..=9 => (0..=rng.below(10))
            .map(|_| vec![moved(rng.place(size))])
            .collect(),
        // The wheel: pans, sideways with Shift, zooms with Ctrl.
        10..=11 => {
            let at = rng.place(size);
            let mut frames = vec![vec![moved(at)]];
            for _ in 0..=rng.below(10) {
                frames.push(vec![wheel(rng.delta(), rng.delta(), rng.below(2) == 0)]);
            }
            frames
        }
        // A shortcut, or any key.
        12..=15 => {
            let events = if rng.below(3) == 0 {
                let (named, code) = rng.pick(&NAMED);
                tap(Key::Named(named), code, None, modifiers)
            } else {
                let (c, code) = rng.pick(&CHARACTERS);
                tap(Key::Character(c.into()), code, Some(c), modifiers)
            };
            vec![events.to_vec()]
        }
        // Typing, into the text overlay when one is open.
        16..=17 => (0..=rng.below(12))
            .map(|_| {
                let typed = rng.pick(&TYPED);
                tap(
                    Key::Character(typed.into()),
                    Code::KeyA,
                    Some(typed),
                    Modifiers::default(),
                )
                .to_vec()
            })
            .collect(),
        // A resize.
        18 => {
            let (width, height) = rng.pick(&SIZES);
            window.size = Size::new(width, height);
            vec![Vec::new()]
        }
        // Far away: a fling of large wheel deltas, often zoomed out first.
        _ => {
            let centre = Point::new(size.width / 2.0, size.height / 2.0);
            let mut frames = vec![vec![moved(centre)]];
            if rng.below(2) == 0 {
                frames.push(vec![
                    Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
                    wheel(0.0, -1e5, false),
                    Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
                ]);
            }
            let (x, y) = (rng.far(), rng.far());
            for _ in 0..=rng.below(20) {
                frames.push(vec![wheel(x, y, false)]);
            }
            frames
        }
    };
    first.append(&mut frames[0]);
    frames[0] = first;
    frames
}

fn moved(position: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position })
}

fn press(button: Button) -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(button))
}

fn release(button: Button) -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(button))
}

fn wheel(x: f32, y: f32, lines: bool) -> Event {
    let delta = if lines {
        ScrollDelta::Lines { x, y }
    } else {
        ScrollDelta::Pixels { x, y }
    };
    Event::Mouse(mouse::Event::WheelScrolled { delta })
}

/// A key pressed and released.
fn tap(key: Key, code: Code, text: Option<&str>, modifiers: Modifiers) -> [Event; 2] {
    let physical_key = Physical::Code(code);
    [
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key.clone(),
            physical_key,
            location: Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        }),
        Event::Keyboard(keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: Location::Standard,
            modifiers,
        }),
    ]
}

/// SplitMix64, as in `tests/editor_fuzz.rs`.
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

    /// A number in `0..max`.
    fn unit(&mut self, max: f32) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32 * max
    }

    /// A window position: mostly anywhere in it, often over the tool bar
    /// at the top, sometimes just outside.
    fn place(&mut self, size: Size) -> Point {
        match self.below(10) {
            0..=6 => Point::new(self.unit(size.width), self.unit(size.height)),
            7..=8 => Point::new(self.unit(size.width), self.unit(60.0)),
            _ => Point::new(
                self.unit(size.width + 80.0) - 40.0,
                self.unit(size.height + 80.0) - 40.0,
            ),
        }
    }

    /// A pointer step from `at`, staying near the window.
    fn near(&mut self, at: Point, size: Size) -> Point {
        Point::new(
            (at.x + self.unit(120.0) - 60.0).clamp(-40.0, size.width + 40.0),
            (at.y + self.unit(120.0) - 60.0).clamp(-40.0, size.height + 40.0),
        )
    }

    /// A wheel delta: mostly a notch or a swipe, sometimes a fling.
    fn delta(&mut self) -> f32 {
        let size = match self.below(20) {
            0..=13 => self.unit(120.0),
            14..=18 => self.unit(2000.0),
            _ => self.unit(1e5),
        };
        if self.below(2) == 0 { size } else { -size }
    }

    /// A wheel delta that pans far (1e4 to 1e6 pixels).
    fn far(&mut self) -> f32 {
        let size = 10f32.powf(4.0 + self.unit(2.0));
        if self.below(2) == 0 { size } else { -size }
    }

    /// Modifier keys: mostly none.
    fn modifiers(&mut self) -> Modifiers {
        if self.below(2) == 0 {
            return Modifiers::default();
        }
        let mut modifiers = Modifiers::default();
        for flag in [Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT] {
            if self.below(2) == 0 {
                modifiers |= flag;
            }
        }
        modifiers
    }
}
