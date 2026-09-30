//! The canvas grid's settings in `appState` (REFERENCE-001 section 17).
use serde_json::{Map, Value};

use super::Scene;

/// The canvas grid: lines every `size` scene units, a bold one every
/// `step` lines. Drawing and moving snap to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// Scene units between lines (`gridSize`).
    pub size: f64,
    /// Lines from one bold line to the next (`gridStep`); 1 draws none bold.
    pub step: u32,
}

// Public API
impl Scene {
    /// Returns the grid while `appState.gridModeEnabled` is on.
    ///
    /// `gridSize` and `gridStep` are rounded and kept within 1 to 100, 20
    /// and 5 when missing, like Excalidraw's restore.
    pub fn grid(&self) -> Option<Grid> {
        let state = self.json.get("appState")?;
        if state.get("gridModeEnabled").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let setting = |key: &str, default: f64| {
            let value = state
                .get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .unwrap_or(default);
            value.round().clamp(1.0, 100.0)
        };
        Some(Grid {
            size: setting("gridSize", 20.0),
            step: setting("gridStep", 5.0) as u32,
        })
    }

    /// Turns the grid on or off (`appState.gridModeEnabled`, saved with the
    /// scene like Excalidraw does).
    pub fn set_grid(&mut self, on: bool) {
        let state = self
            .json
            .entry("appState")
            .or_insert_with(|| Value::Object(Map::new()));
        if !state.is_object() {
            *state = Value::Object(Map::new());
        }
        state
            .as_object_mut()
            .expect("made an object above")
            .insert("gridModeEnabled".into(), on.into());
    }
}
