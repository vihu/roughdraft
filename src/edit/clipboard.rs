//! Copy, cut and paste as Excalidraw clipboard JSON (`clipboard.ts`,
//! `App.addElementsFromPasteOrLibrary`), so content moves between this
//! editor and web Excalidraw.
use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::{Editor, common_bounds};
use crate::geometry::Point;
use crate::hit::container_id;
use crate::scene::Element;

/// `EXPORT_DATA_TYPES.excalidrawClipboard`.
const CLIPBOARD_TYPE: &str = "excalidraw/clipboard";

/// Other payloads that carry pasteable elements: the programmatic clipboard
/// and a whole scene file.
const PASTEABLE_TYPES: [&str; 3] = [CLIPBOARD_TYPE, "excalidraw-api/clipboard", "excalidraw"];

impl Editor {
    /// Returns the selection and its labels as Excalidraw clipboard JSON, or
    /// `None` when nothing is selected.
    pub fn copy(&self) -> Option<String> {
        if self.selected.is_empty() {
            return None;
        }
        let elements: Vec<&Element> = self
            .moving()
            .into_iter()
            .map(|(i, _)| &self.scene.elements[i])
            .collect();
        // ponytail: no `files` yet; images come with slice 09
        let clipboard = json!({ "type": CLIPBOARD_TYPE, "elements": elements });
        Some(clipboard.to_string())
    }

    /// Copies the selection, then deletes it.
    pub fn cut(&mut self) -> Option<String> {
        let json = self.copy()?;
        self.delete();
        Some(json)
    }

    /// Pastes Excalidraw clipboard (or scene) JSON with its box centred on
    /// `at`. Copies get new ids and seeds, go on top, and become the
    /// selection. Returns `false` when `text` holds no elements.
    pub fn paste(&mut self, text: &str, at: Point) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(text.trim()) else {
            return false;
        };
        if !value["type"]
            .as_str()
            .is_some_and(|t| PASTEABLE_TYPES.contains(&t))
        {
            return false;
        }
        let mut elements: Vec<Element> = value["elements"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|e| serde_json::from_value(e.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        elements.retain(|e: &Element| !e.base.is_deleted);
        if elements.is_empty() {
            return false;
        }

        let [x1, y1, x2, y2] = common_bounds(elements.iter());
        let offset = [at[0] - (x2 - x1) / 2.0 - x1, at[1] - (y2 - y1) / 2.0 - y1];
        let ids: HashMap<String, String> = elements
            .iter()
            .map(|e| (e.base.id.clone(), crate::random::id()))
            .collect();
        let mut groups = HashMap::new();
        for element in &mut elements {
            let mut copy = element.duplicate();
            copy.base.id = ids[&element.base.id].clone();
            copy.base.x += offset[0];
            copy.base.y += offset[1];
            copy.remap(&ids, &mut groups);
            *element = copy;
        }

        self.history.record(self.scene.elements.clone());
        self.selected = elements
            .iter()
            .filter(|e| container_id(e).is_none())
            .map(|e| e.base.id.clone())
            .collect::<HashSet<_>>();
        self.scene.elements.extend(elements);
        true
    }
}
