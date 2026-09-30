//! Excalidraw's keyboard shortcuts as editor commands.
use iced::keyboard::{self, Key, key::Named};

use crate::edit::{Command, Order, Tool};

/// Maps Excalidraw's shortcuts to editor commands.
pub(super) fn shortcut(key: &Key, modifiers: keyboard::Modifiers) -> Option<Command> {
    /// Arrow-key nudge in scene units: plain, and with Shift.
    const NUDGE: (f64, f64) = (1.0, 5.0);

    let (command, shift, alt) = (modifiers.command(), modifiers.shift(), modifiers.alt());
    let step = if shift { NUDGE.1 } else { NUDGE.0 };
    match key.as_ref() {
        Key::Named(Named::Delete | Named::Backspace) if !command => Some(Command::Delete),
        Key::Named(Named::Escape) => Some(Command::Escape),
        Key::Named(Named::Enter) if command => Some(Command::EditLine),
        Key::Named(Named::Enter) => Some(Command::Finish),
        Key::Named(Named::ArrowLeft) => Some(Command::Nudge([-step, 0.0])),
        Key::Named(Named::ArrowRight) => Some(Command::Nudge([step, 0.0])),
        Key::Named(Named::ArrowUp) => Some(Command::Nudge([0.0, -step])),
        Key::Named(Named::ArrowDown) => Some(Command::Nudge([0.0, step])),
        Key::Character(c) => match (c.to_lowercase().as_str(), command) {
            ("z", true) if shift => Some(Command::Redo),
            ("z", true) => Some(Command::Undo),
            ("y", true) => Some(Command::Redo),
            ("d", true) => Some(Command::Duplicate),
            ("a", true) => Some(Command::SelectAll),
            ("g", true) if shift => Some(Command::Ungroup),
            // `,` and `.` for layouts where Shift does not make `<` and `>`.
            ("<" | ",", true) if shift => Some(Command::SmallerFont),
            (">" | ".", true) if shift => Some(Command::LargerFont),
            ("g", true) => Some(Command::Group),
            // Shift turns the brackets into braces on most layouts.
            ("[" | "{", true) if shift => Some(Command::Reorder(Order::ToBack)),
            ("[", true) => Some(Command::Reorder(Order::Backward)),
            ("]" | "}", true) if shift => Some(Command::Reorder(Order::ToFront)),
            ("]", true) => Some(Command::Reorder(Order::Forward)),
            (_, true) => None,
            _ if alt => None,
            ("v" | "1", _) => Some(Command::Tool(Tool::Selection)),
            ("h", _) => Some(Command::Tool(Tool::Hand)),
            ("r" | "2", _) => Some(Command::Tool(Tool::Rectangle)),
            ("d" | "3", _) => Some(Command::Tool(Tool::Diamond)),
            ("o" | "4", _) => Some(Command::Tool(Tool::Ellipse)),
            ("a" | "5", _) => Some(Command::Tool(Tool::Arrow)),
            ("l" | "6", _) => Some(Command::Tool(Tool::Line)),
            ("t" | "8", _) => Some(Command::Tool(Tool::Text)),
            ("q", _) => Some(Command::ToggleLock),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::{Command, Tool, shortcut};

    #[test]
    fn shortcuts_follow_excalidraw() {
        let ctrl = Modifiers::CTRL;
        let key = |c: &str| Key::Character(c.into());
        assert_eq!(shortcut(&key("z"), ctrl), Some(Command::Undo));
        assert_eq!(
            shortcut(&key("Z"), ctrl | Modifiers::SHIFT),
            Some(Command::Redo)
        );
        assert_eq!(shortcut(&key("d"), ctrl), Some(Command::Duplicate));
        assert_eq!(
            shortcut(&key("d"), Modifiers::empty()),
            Some(Command::Tool(Tool::Diamond))
        );
        assert_eq!(
            shortcut(&key("5"), Modifiers::empty()),
            Some(Command::Tool(Tool::Arrow))
        );
        assert_eq!(
            shortcut(&key("h"), Modifiers::empty()),
            Some(Command::Tool(Tool::Hand))
        );
        assert_eq!(
            shortcut(&key("d"), Modifiers::ALT | Modifiers::SHIFT),
            None,
            "theme toggle stays with the app"
        );
        assert_eq!(
            shortcut(&Key::Named(Named::ArrowLeft), Modifiers::SHIFT),
            Some(Command::Nudge([-5.0, 0.0]))
        );
        assert_eq!(
            shortcut(&key("<"), ctrl | Modifiers::SHIFT),
            Some(Command::SmallerFont)
        );
        assert_eq!(
            shortcut(&key("."), ctrl | Modifiers::SHIFT),
            Some(Command::LargerFont)
        );
    }
}
