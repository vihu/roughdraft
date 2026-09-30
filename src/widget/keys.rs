//! Excalidraw's keyboard shortcuts as editor commands.
use iced::keyboard::{self, Key, key::Named};

use crate::edit::{Axis, Command, Order, Tool};

/// Shortcuts Excalidraw matches by key code, so they work whatever Alt or
/// the layout makes of the key: copy and paste styles, and the grid.
pub(super) fn code_shortcut(
    code: keyboard::key::Code,
    modifiers: keyboard::Modifiers,
) -> Option<Command> {
    use keyboard::key::Code;
    match (code, modifiers.command(), modifiers.alt()) {
        (Code::KeyC, true, true) => Some(Command::CopyStyles),
        (Code::KeyV, true, true) => Some(Command::PasteStyles),
        (Code::Quote, true, false) => Some(Command::ToggleGrid),
        _ => None,
    }
}

/// Maps Excalidraw's shortcuts to editor commands. `grid` is the grid size
/// while the grid is on.
pub(super) fn shortcut(
    key: &Key,
    modifiers: keyboard::Modifiers,
    grid: Option<f64>,
) -> Option<Command> {
    /// Arrow-key nudge in scene units: plain, and with Shift.
    const NUDGE: (f64, f64) = (1.0, 5.0);

    let (command, shift, alt) = (modifiers.command(), modifiers.shift(), modifiers.alt());
    // On the grid a step is one cell, and Shift a single unit.
    let step = match (grid, shift) {
        (Some(_), true) => NUDGE.0,
        (Some(size), false) => size,
        (None, true) => NUDGE.1,
        (None, false) => NUDGE.0,
    };
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
            ("l", true) if shift => Some(Command::ToggleElementLock),
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
            ("h", _) if shift => Some(Command::Flip(Axis::Horizontal)),
            ("v", _) if shift => Some(Command::Flip(Axis::Vertical)),
            ("v" | "1", _) => Some(Command::Tool(Tool::Selection)),
            ("h", _) => Some(Command::Tool(Tool::Hand)),
            ("r" | "2", _) => Some(Command::Tool(Tool::Rectangle)),
            ("d" | "3", _) => Some(Command::Tool(Tool::Diamond)),
            ("o" | "4", _) => Some(Command::Tool(Tool::Ellipse)),
            ("a" | "5", _) => Some(Command::Tool(Tool::Arrow)),
            ("l" | "6", _) => Some(Command::Tool(Tool::Line)),
            ("p" | "x" | "7", _) => Some(Command::Tool(Tool::Freedraw)),
            ("t" | "8", _) => Some(Command::Tool(Tool::Text)),
            ("e" | "0", _) => Some(Command::Tool(Tool::Eraser)),
            ("f", _) => Some(Command::Tool(Tool::Frame)),
            ("q", _) => Some(Command::ToggleLock),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::{Command, Tool, code_shortcut, shortcut};

    #[test]
    fn key_code_shortcuts_copy_styles_and_toggle_the_grid() {
        use iced::keyboard::key::Code;
        let command_alt = Modifiers::COMMAND | Modifiers::ALT;
        assert_eq!(
            code_shortcut(Code::KeyC, command_alt),
            Some(Command::CopyStyles)
        );
        assert_eq!(
            code_shortcut(Code::KeyV, command_alt),
            Some(Command::PasteStyles)
        );
        assert_eq!(
            code_shortcut(Code::KeyC, Modifiers::COMMAND),
            None,
            "plain copy"
        );
        assert_eq!(
            code_shortcut(Code::Quote, Modifiers::COMMAND),
            Some(Command::ToggleGrid)
        );
    }

    #[test]
    fn shortcuts_follow_excalidraw() {
        let command = Modifiers::COMMAND;
        let key = |c: &str| Key::Character(c.into());
        assert_eq!(shortcut(&key("z"), command, None), Some(Command::Undo));
        assert_eq!(
            shortcut(&key("Z"), command | Modifiers::SHIFT, None),
            Some(Command::Redo)
        );
        assert_eq!(shortcut(&key("d"), command, None), Some(Command::Duplicate));
        assert_eq!(
            shortcut(&key("L"), command | Modifiers::SHIFT, None),
            Some(Command::ToggleElementLock)
        );
        assert_eq!(
            shortcut(&key("d"), Modifiers::empty(), None),
            Some(Command::Tool(Tool::Diamond))
        );
        assert_eq!(
            shortcut(&key("5"), Modifiers::empty(), None),
            Some(Command::Tool(Tool::Arrow))
        );
        assert_eq!(
            shortcut(&key("h"), Modifiers::empty(), None),
            Some(Command::Tool(Tool::Hand))
        );
        assert_eq!(
            shortcut(&key("H"), Modifiers::SHIFT, None),
            Some(Command::Flip(crate::edit::Axis::Horizontal))
        );
        assert_eq!(
            shortcut(&key("d"), Modifiers::ALT | Modifiers::SHIFT, None),
            None,
            "theme toggle stays with the app"
        );
        assert_eq!(
            shortcut(&Key::Named(Named::ArrowLeft), Modifiers::SHIFT, None),
            Some(Command::Nudge([-5.0, 0.0]))
        );
        // On a 20-unit grid: a cell, and a single unit with Shift.
        assert_eq!(
            shortcut(&Key::Named(Named::ArrowUp), Modifiers::empty(), Some(20.0)),
            Some(Command::Nudge([0.0, -20.0]))
        );
        assert_eq!(
            shortcut(&Key::Named(Named::ArrowUp), Modifiers::SHIFT, Some(20.0)),
            Some(Command::Nudge([0.0, -1.0]))
        );
        assert_eq!(
            shortcut(&key("<"), command | Modifiers::SHIFT, None),
            Some(Command::SmallerFont)
        );
        assert_eq!(
            shortcut(&key("."), command | Modifiers::SHIFT, None),
            Some(Command::LargerFont)
        );
    }
}
