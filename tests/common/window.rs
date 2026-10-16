//! The app's window, headless: `Sketch::view` driven frame by frame the way
//! the app's event loop does, with widget state (focus, scroll) kept between
//! frames. Shared by `tests/sketch_widget.rs` and `examples/stress.rs`.
// Each user takes a different subset of these items.
#![allow(dead_code)]
use iced::advanced::renderer::{self, Headless};
use iced::advanced::shell::{Bus, Waker};
use iced::futures::{FutureExt, StreamExt};
use iced::keyboard::Modifiers;
use iced::mouse;
use iced::theme::Base;
use iced::{Color, Event, Point, Size, Task};
use iced_test::core::{time, window};
use iced_test::runtime::user_interface::{self, UserInterface};
use iced_test::runtime::{Action, task};
use roughdraft::widget::{Message, Sketch};

/// A headless renderer: `backend` (`wgpu` or `tiny-skia`), or iced's
/// choice.
pub fn renderer(backend: Option<&str>) -> iced::Renderer {
    let renderer = iced::Renderer::new(renderer::Settings::default(), backend);
    iced::futures::executor::block_on(renderer).expect("a headless renderer")
}

/// The window's size, pointer, modifier keys and the interface's state
/// between frames.
pub struct Window {
    pub size: Size,
    pub cursor: Point,
    pub modifiers: Modifiers,
    cache: user_interface::Cache,
    bus: Bus<Message>,
}

impl Window {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            cursor: Point::ORIGIN,
            modifiers: Modifiers::default(),
            cache: user_interface::Cache::default(),
            bus: Bus::new(),
        }
    }

    /// One frame as the app's event loop runs it: the events, the messages
    /// they published, the rebuilt interface with the tasks' widget
    /// operations, then the redraw. `present` also rasterizes the frame
    /// (tiny-skia only: wgpu's headless screenshot panics once geometry
    /// changes, see `examples/bench.rs`).
    pub fn frame(
        &mut self,
        sketch: &mut Sketch,
        renderer: &mut iced::Renderer,
        events: &[Event],
        present: bool,
    ) {
        let cache = std::mem::take(&mut self.cache);
        let mut ui = UserInterface::build(sketch.view(), self.size, cache, renderer);
        for event in events {
            if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
                self.cursor = *position;
            }
            let _ = ui.update(
                &window::Headless,
                &Waker::noop(),
                std::slice::from_ref(event),
                mouse::Cursor::Available(self.cursor),
                renderer,
                &mut self.bus,
            );
        }
        self.cache = ui.into_cache();
        let tasks: Vec<Task<Message>> = self
            .bus
            .drain()
            .map(|(message, _)| sketch.update(message))
            .collect();

        let cache = std::mem::take(&mut self.cache);
        let mut ui = UserInterface::build(sketch.view(), self.size, cache, renderer);
        for task in tasks {
            let Some(mut actions) = task::into_stream(task) else {
                continue;
            };
            // Clipboard actions never get their answer, which ends the task.
            while let Some(Some(action)) = actions.next().now_or_never() {
                if let Action::Widget(mut operation) = action {
                    ui.operate(renderer, operation.as_mut());
                }
            }
        }
        let cursor = mouse::Cursor::Available(self.cursor);
        let redraw = Event::Window(window::Event::RedrawRequested(time::Instant::now()));
        let _ = ui.update(
            &window::Headless,
            &Waker::noop(),
            &[redraw],
            cursor,
            renderer,
            &mut self.bus,
        );
        let theme = iced::Theme::Light;
        let style = renderer::Style {
            text_color: theme.base().text_color,
        };
        ui.draw(renderer, &theme, &style, cursor);
        self.cache = ui.into_cache();
        if present {
            let size = Size::new(self.size.width as u32, self.size.height as u32);
            let _pixels = renderer.screenshot(size, 1.0, Color::WHITE);
        }
    }
}
