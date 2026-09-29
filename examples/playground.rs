//! Playground: open, view and (later) edit `.excalidraw` files from disk.
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Geometry};
use iced::{Color, Element, Fill, Point, Rectangle, Renderer, Theme};

pub fn main() -> iced::Result {
    iced::application(Playground::default, Playground::update, Playground::view)
        .title("roughdraft playground")
        .run()
}

#[derive(Default)]
struct Playground {
    scene: canvas::Cache,
}

#[derive(Debug, Clone, Copy)]
enum Message {}

impl Playground {
    fn update(&mut self, message: Message) {
        match message {}
    }

    fn view(&self) -> Element<'_, Message> {
        Canvas::new(self).width(Fill).height(Fill).into()
    }
}

impl canvas::Program<Message> for Playground {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![self.scene.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(Point::ORIGIN, frame.size(), Color::WHITE);
        })]
    }
}
