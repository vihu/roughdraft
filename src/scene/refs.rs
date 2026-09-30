//! References between elements: labels, bound arrows, groups, frames.
use std::collections::HashMap;

use serde_json::Value;

use super::{Element, Kind};

impl Element {
    /// Points this copy's references at other copies, after the caller gave
    /// every copied element a new id (`ids`, old to new).
    ///
    /// References to elements that were not copied are dropped, as
    /// Excalidraw does on paste and duplicate. Group ids get new ids too,
    /// shared by all copies in the same operation (`groups` collects them).
    pub fn remap(&mut self, ids: &HashMap<String, String>, groups: &mut HashMap<String, String>) {
        if let Kind::Text(text) = &mut self.kind {
            text.container_id = text
                .container_id
                .take()
                .and_then(|id| ids.get(&id).cloned());
        }
        if let Some(bound) = &mut self.base.bound_elements {
            bound.retain(|b| ids.contains_key(&b.id));
            bound.iter_mut().for_each(|b| b.id = ids[&b.id].clone());
        }
        for key in ["startBinding", "endBinding"] {
            let target = self
                .json
                .get(key)
                .and_then(|b| b.get("elementId"))
                .and_then(Value::as_str);
            match target.map(|id| ids.get(id)) {
                Some(Some(new)) => {
                    let new = Value::String(new.clone());
                    self.json
                        .get_mut(key)
                        .and_then(Value::as_object_mut)
                        .map(|b| b.insert("elementId".into(), new));
                }
                Some(None) => {
                    self.json.insert(key.into(), Value::Null);
                }
                None => {}
            }
        }
        if let Some(Value::Array(group_ids)) = self.json.get_mut("groupIds") {
            for group in group_ids.iter_mut() {
                if let Value::String(id) = group {
                    *id = groups
                        .entry(id.clone())
                        .or_insert_with(crate::random::id)
                        .clone();
                }
            }
        }
        if let Some(Value::String(frame)) = self.json.get("frameId") {
            let frame = ids
                .get(frame)
                .map_or(Value::Null, |new| Value::String(new.clone()));
            self.json.insert("frameId".into(), frame);
        }
    }

    /// Returns the ids of the groups this element belongs to, innermost
    /// first.
    pub fn group_ids(&self) -> Vec<&str> {
        match self.json.get("groupIds") {
            Some(Value::Array(ids)) => ids.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::scene::{Element, Kind};

    #[test]
    fn remap_rewires_copied_references_and_drops_the_rest() {
        let json = r##"{"id":"t","type":"text","x":0,"y":0,"width":10,"height":10,"angle":0,"strokeColor":"#000","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":1,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"isDeleted":false,"groupIds":["g1","g2"],"frameId":"f","boundElements":[{"id":"a","type":"arrow"},{"id":"gone","type":"arrow"}],"startBinding":{"elementId":"box","focus":0,"gap":1},"endBinding":{"elementId":"gone","focus":0,"gap":1},"text":"x","fontSize":20,"fontFamily":5,"textAlign":"left","verticalAlign":"top","lineHeight":1.25,"containerId":"box"}"##;
        let mut element: Element = serde_json::from_str(json).unwrap();
        let ids: HashMap<String, String> = [("box", "box2"), ("a", "a2")]
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .into();
        let mut groups = HashMap::new();
        element.remap(&ids, &mut groups);

        let Kind::Text(text) = &element.kind else {
            panic!("text")
        };
        assert_eq!(text.container_id.as_deref(), Some("box2"));
        let bound = element.base.bound_elements.as_ref().unwrap();
        assert_eq!((bound.len(), bound[0].id.as_str()), (1, "a2"));
        let out = serde_json::to_value(&element).unwrap();
        assert_eq!(out["startBinding"]["elementId"], "box2");
        assert!(out["endBinding"].is_null() && out["frameId"].is_null());
        let group_ids = element.group_ids();
        assert_eq!(group_ids, [groups["g1"].as_str(), groups["g2"].as_str()]);
        assert_ne!(group_ids[0], "g1");
    }
}
