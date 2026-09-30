//! Undo and redo as element snapshots. Only the top of each stack is kept
//! whole; the ones below keep just the elements that differ from the
//! snapshot above them, so a long session costs what it changed, not a full
//! copy of the scene per action.
use std::collections::HashMap;

use crate::scene::Element;

/// Undo and redo stacks of element lists.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// A stack entry: whole on top, relative to the entry above otherwise.
#[derive(Clone, Debug)]
enum Snapshot {
    Full(Vec<Element>),
    Delta(Vec<Entry>),
}

/// One element of a delta snapshot.
#[derive(Clone, Debug)]
enum Entry {
    /// Equal to the element at this index of the snapshot above.
    Same(usize),
    /// Boxed, so unchanged entries stay small.
    Changed(Box<Element>),
}

impl History {
    /// Records the elements as they were before an action; clears redo.
    pub fn record(&mut self, before: Vec<Element>) {
        push(&mut self.undo, before);
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

    fn swap(from: &mut Vec<Snapshot>, to: &mut Vec<Snapshot>, elements: &mut Vec<Element>) -> bool {
        let Some(Snapshot::Full(snapshot)) = from.pop() else {
            return false;
        };
        push(to, std::mem::replace(elements, snapshot));
        // The next entry down was relative to what is now restored.
        if let Some(Snapshot::Delta(entries)) = from.pop() {
            from.push(Snapshot::Full(resolve(entries, elements)));
        }
        true
    }
}

/// Pushes a whole snapshot, turning the old top into a delta against it.
fn push(stack: &mut Vec<Snapshot>, top: Vec<Element>) {
    if let Some(Snapshot::Full(older)) = stack.pop() {
        stack.push(Snapshot::Delta(delta(older, &top)));
    }
    stack.push(Snapshot::Full(top));
}

/// `older` described against `newer`: unchanged elements become indices.
fn delta(older: Vec<Element>, newer: &[Element]) -> Vec<Entry> {
    let at: HashMap<&str, usize> = newer
        .iter()
        .enumerate()
        .map(|(i, e)| (e.base.id.as_str(), i))
        .collect();
    older
        .into_iter()
        .map(|element| match at.get(element.base.id.as_str()) {
            Some(&i) if newer[i] == element => Entry::Same(i),
            _ => Entry::Changed(Box::new(element)),
        })
        .collect()
}

fn resolve(entries: Vec<Entry>, newer: &[Element]) -> Vec<Element> {
    entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Same(i) => newer[i].clone(),
            Entry::Changed(element) => *element,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{History, Snapshot};
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

    #[test]
    fn older_snapshots_keep_only_what_changed_and_restore_whole() {
        let json = r##"{"elements":[
            {"id":"a","type":"rectangle","x":0,"y":0,"width":1,"height":1,"angle":0,"strokeColor":"#000","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":1,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"isDeleted":false},
            {"id":"b","type":"rectangle","x":5,"y":0,"width":1,"height":1,"angle":0,"strokeColor":"#000","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":1,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":2,"isDeleted":false}
        ]}"##;
        let mut elements = serde_json::from_str::<Scene>(json).unwrap().elements;
        let mut history = History::default();
        let mut states = vec![elements.clone()];
        for step in 1..=4 {
            history.record(elements.clone());
            elements[0].base.x = f64::from(step);
            if step == 3 {
                elements.swap(0, 1);
            }
            states.push(elements.clone());
        }
        // Only a's change is stored below the top.
        let Snapshot::Delta(entries) = &history.undo[1] else {
            panic!("below the top is a delta")
        };
        assert_eq!(
            entries
                .iter()
                .filter(|e| matches!(e, super::Entry::Changed(_)))
                .count(),
            1
        );

        for state in states.iter().rev().skip(1) {
            assert!(history.undo(&mut elements));
            assert_eq!(&elements, state);
        }
        assert!(!history.undo(&mut elements));
        for state in states.iter().skip(1) {
            assert!(history.redo(&mut elements));
            assert_eq!(&elements, state);
        }
    }
}
