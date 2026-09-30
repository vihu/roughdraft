//! Frame times for the milestone exit checks (PLAN-001): builds a scene of
//! mixed elements with the editor, then drags one element and pans the view
//! through the canvas' own input path, drawing each frame with a headless
//! renderer.
//!
//! ```text
//! cargo run --release --example bench -- [elements] [wgpu|tiny-skia]
//! ```
//!
//! Per frame it times `update` (input to editor state), `draw` (rough
//! geometry, plus tessellation into meshes on wgpu) and, on tiny-skia only,
//! `present` (software rasterization, the whole frame on that backend). On
//! wgpu the GPU part is left out: iced's headless screenshot panics in wgpu
//! ("StagingBelt staging buffer is still mapped") once geometry changes
//! between two screenshots.
use std::time::{Duration, Instant};

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::{self, Headless};
use iced::keyboard::{self, Key};
use iced::widget::canvas::Program;
use iced::{Color, Event, Point, Rectangle, Size, mouse};
use roughdraft::edit::{Command, Editor, Modifiers, Pointer, StyleChange, Tool};
use roughdraft::scene::{FillStyle, Scene};
use roughdraft::widget::{Message, Sketch};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const FRAMES: usize = 120;

fn main() {
    let mut args = std::env::args().skip(1);
    let count: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(500);
    let backend = args.next().unwrap_or_else(|| "wgpu".into());
    let scene = build_scene(count);

    let start = Instant::now();
    let mut sketch = Sketch::new(scene);
    // Screen pixels are scene units, so the press below lands on element 0.
    sketch.set_origin([0.0, 0.0]);
    let renderer = iced::Renderer::new(renderer::Settings::default(), Some(&backend));
    let mut bench = Bench {
        renderer: iced::futures::executor::block_on(renderer).expect("a headless renderer"),
        state: Default::default(),
        present: backend != "wgpu",
    };
    bench.frame(&mut sketch, None, mouse::Cursor::Unavailable);
    println!(
        "{} elements, {}: first frame {:.1} ms (widget with fonts, renderer, full draw)",
        sketch.scene().elements.len(),
        bench.renderer.name(),
        start.elapsed().as_secs_f64() * 1e3,
    );

    // Drag the first element (a rectangle at 20..120, 20..80) to the right.
    let press = Point::new(20.0, 50.0);
    let down = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    bench.frame(&mut sketch, Some(&down), mouse::Cursor::Available(press));
    let drag: Vec<Timing> = (1..=FRAMES)
        .map(|i| {
            let position = Point::new(press.x + i as f32 * 2.0, press.y);
            let moved = Event::Mouse(mouse::Event::CursorMoved { position });
            bench.frame(
                &mut sketch,
                Some(&moved),
                mouse::Cursor::Available(position),
            )
        })
        .collect();
    let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    bench.frame(&mut sketch, Some(&up), mouse::Cursor::Available(press));
    let moved = sketch.scene().elements[0].base.x;
    assert_eq!(
        moved,
        20.0 + 2.0 * FRAMES as f64,
        "the drag moved the first element"
    );
    report("drag one element", &drag);

    let centre = mouse::Cursor::Available(Rectangle::with_size(VIEWPORT).center());
    let pan: Vec<Timing> = (0..FRAMES)
        .map(|_| {
            let wheel = Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -4.0 },
            });
            bench.frame(&mut sketch, Some(&wheel), centre)
        })
        .collect();
    report("pan", &pan);

    // The same pan with the grid on (Ctrl+'), which is redrawn with the view.
    let grid = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("'".into()),
        modified_key: Key::Character("'".into()),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Quote),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::COMMAND,
        text: None,
        repeat: false,
    });
    bench.frame(&mut sketch, Some(&grid), centre);
    assert!(sketch.scene().grid().is_some(), "Ctrl+' turned the grid on");
    let pan: Vec<Timing> = (0..FRAMES)
        .map(|_| {
            let wheel = Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -4.0 },
            });
            bench.frame(&mut sketch, Some(&wheel), centre)
        })
        .collect();
    report("pan with the grid", &pan);
    bench.frame(&mut sketch, Some(&grid), centre);

    // One long pen stroke, 8 pointer events per frame (a 500 Hz mouse at
    // 60 frames a second): the cost grows with its length.
    let key = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("p".into()),
        modified_key: Key::Character("p".into()),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyP),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::default(),
        text: Some("p".into()),
        repeat: false,
    });
    bench.frame(&mut sketch, Some(&key), centre);
    let at = |i: usize| {
        let t = i as f32 / 40.0;
        Point::new(100.0 + i as f32 * 0.5, 400.0 + 150.0 * t.sin())
    };
    bench.frame(&mut sketch, Some(&down), mouse::Cursor::Available(at(0)));
    let stroke: Vec<Timing> = (1..=STROKE / PER_FRAME)
        .map(|frame| {
            let events: Vec<_> = (1..=PER_FRAME)
                .map(|k| {
                    let position = at((frame - 1) * PER_FRAME + k);
                    let moved = Event::Mouse(mouse::Event::CursorMoved { position });
                    (moved, mouse::Cursor::Available(position))
                })
                .collect();
            let cursor = mouse::Cursor::Available(at(frame * PER_FRAME));
            bench.frame_of(&mut sketch, &events, cursor)
        })
        .collect();
    let count = sketch.scene().elements.len();
    bench.frame(&mut sketch, Some(&up), mouse::Cursor::Available(at(STROKE)));
    assert_eq!(sketch.scene().elements.len(), count, "the stroke was drawn");
    let tail = 200 / PER_FRAME;
    report("pen, first 200 points", &stroke[..tail]);
    report(
        &format!("pen, points {} to {STROKE}", STROKE - 200),
        &stroke[stroke.len() - tail..],
    );
}

/// Points in the pen stroke.
const STROKE: usize = 2000;

/// Pointer events between two frames while drawing the stroke.
const PER_FRAME: usize = 8;

/// The canvas' state and renderer, kept across frames like a window's.
struct Bench {
    renderer: iced::Renderer,
    state: <Sketch as Program<Message>>::State,
    present: bool,
}

/// Time spent in each phase of one frame.
#[derive(Clone, Copy, Default)]
struct Timing {
    update: Duration,
    draw: Duration,
    present: Duration,
}

impl Bench {
    /// Feeds one event through the canvas and the sketch, then draws a frame.
    fn frame(
        &mut self,
        sketch: &mut Sketch,
        event: Option<&Event>,
        cursor: mouse::Cursor,
    ) -> Timing {
        self.frame_of(
            sketch,
            event.map(|e| (e.clone(), cursor)).as_slice(),
            cursor,
        )
    }

    /// Feeds several events, as a window gets from a fast mouse between two
    /// frames, then draws one frame with the cursor at `cursor`.
    fn frame_of(
        &mut self,
        sketch: &mut Sketch,
        events: &[(Event, mouse::Cursor)],
        cursor: mouse::Cursor,
    ) -> Timing {
        let bounds = Rectangle::with_size(VIEWPORT);
        let start = Instant::now();
        for (event, cursor) in events {
            let action = Program::update(&*sketch, &mut self.state, event, bounds, *cursor);
            if let Some((Some(message), _, _)) = action.map(|a| a.into_inner()) {
                let _ = sketch.update(message);
            }
        }
        let update = start.elapsed();

        let start = Instant::now();
        let layers = Program::draw(
            &*sketch,
            &self.state,
            &self.renderer,
            &iced::Theme::Light,
            bounds,
            cursor,
        );
        let draw = start.elapsed();

        let start = Instant::now();
        if self.present {
            for layer in layers {
                self.renderer.draw_geometry(layer);
            }
            let size = Size::new(VIEWPORT.width as u32, VIEWPORT.height as u32);
            let _pixels = self.renderer.screenshot(size, 1.0, Color::WHITE);
        }
        let present = start.elapsed();
        Timing {
            update,
            draw,
            present,
        }
    }
}

fn report(name: &str, frames: &[Timing]) {
    let stats = |pick: fn(&Timing) -> Duration| {
        let mut ms: Vec<f64> = frames.iter().map(|t| pick(t).as_secs_f64() * 1e3).collect();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        format!("median {:.2} ms, p95 {:.2} ms", at(0.5), at(0.95))
    };
    println!("{name} ({} frames):", frames.len());
    println!("  update  {}", stats(|t| t.update));
    println!("  draw    {}", stats(|t| t.draw));
    println!("  present {}", stats(|t| t.present));
    println!("  total   {}", stats(|t| t.update + t.draw + t.present));
}

/// A grid of rectangles, ellipses, diamonds (hachure, solid and cross-hatch
/// fills), arrows, lines and text, drawn with the editor like a user would.
fn build_scene(count: usize) -> Scene {
    let mut editor = Editor::new(Scene::default());
    let none = Modifiers::default();
    let columns = 25;
    for i in 0..count {
        let (x, y) = (
            (i % columns) as f64 * 150.0 + 20.0,
            (i / columns) as f64 * 110.0 + 20.0,
        );
        let (tool, fill) = match i % 6 {
            0 => (Tool::Rectangle, Some(FillStyle::Hachure)),
            1 => (Tool::Ellipse, Some(FillStyle::Solid)),
            2 => (Tool::Diamond, Some(FillStyle::CrossHatch)),
            3 => (Tool::Arrow, None),
            4 => (Tool::Line, None),
            _ => (Tool::Text, None),
        };
        // Deselect first, or the style change would restyle the last element.
        editor.command(Command::Tool(Tool::Selection));
        editor.pointer(Pointer::Down, [-1e4, -1e4], none);
        editor.pointer(Pointer::Up, [-1e4, -1e4], none);
        if tool == Tool::Text {
            editor.paste("Hand-drawn label", [x + 50.0, y + 30.0]);
            continue;
        }
        let background = if fill.is_some() {
            "#a5d8ff"
        } else {
            "transparent"
        };
        editor.apply_style(StyleChange::BackgroundColor(background.into()));
        if let Some(fill) = fill {
            editor.apply_style(StyleChange::FillStyle(fill));
        }
        editor.command(Command::Tool(tool));
        editor.pointer(Pointer::Down, [x, y], none);
        editor.pointer(Pointer::Move, [x + 100.0, y + 60.0], none);
        editor.pointer(Pointer::Up, [x + 100.0, y + 60.0], none);
        editor.command(Command::Escape);
    }
    editor.scene().clone()
}
