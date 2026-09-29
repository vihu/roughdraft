//! Undo and redo as whole-scene element snapshots.
use crate::scene::Element;

/// Undo and redo stacks of element lists.
// ponytail: full snapshots per action, store per-element diffs if memory matters
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Vec<Element>>,
    redo: Vec<Vec<Element>>,
}

impl History {
    /// Records the elements as they were before an action; clears redo.
    pub fn record(&mut self, before: Vec<Element>) {
        self.undo.push(before);
        self.redo.clear();
    }

    /// Restores the previous snapshot into `elements`.
    ///
    /// Returns `false` when there is nothing to undo.
    pub fn undo(&mut self, elements: &mut Vec<Element>) -> bool {
        Self::swap(&mut self.undo, &mut self.redo, elements)
    }

    /// Re-applies the last undone snapshot into `elements`.
    ///
    /// Returns `false` when there is nothing to redo.
    pub fn redo(&mut self, elements: &mut Vec<Element>) -> bool {
        Self::swap(&mut self.redo, &mut self.undo, elements)
    }

    fn swap(
        from: &mut Vec<Vec<Element>>,
        to: &mut Vec<Vec<Element>>,
        elements: &mut Vec<Element>,
    ) -> bool {
        let Some(snapshot) = from.pop() else {
            return false;
        };
        to.push(std::mem::replace(elements, snapshot));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::History;
    use crate::scene::Scene;

    #[test]
    fn undo_redo_walks_snapshots() {
        let json = r##"{"elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":1,"height":1,"angle":0,"strokeColor":"#000","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":1,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"isDeleted":false}]}"##;
        let scene: Scene = serde_json::from_str(json).unwrap();
        let mut elements = scene.elements;
        let mut history = History::default();

        history.record(elements.clone());
        elements[0].base.x = 10.0;
        assert!(history.undo(&mut elements));
        assert_eq!(elements[0].base.x, 0.0);
        assert!(history.redo(&mut elements));
        assert_eq!(elements[0].base.x, 10.0);
        assert!(!history.redo(&mut elements));

        history.record(elements.clone());
        assert!(history.undo(&mut elements));
        history.record(elements.clone());
        assert!(!history.redo(&mut elements), "a new action clears redo");
    }
}
